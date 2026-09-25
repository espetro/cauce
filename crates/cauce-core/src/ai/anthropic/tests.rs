//! Hand-authored Anthropic-API fixtures replayed through `wiremock` —
//! hermetic, no live provider calls. The fixtures mirror the Messages
//! API wire format documented at docs.claude.com (event names, `delta`
//! block types, `partial_json` chunks); they are the contract.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::*;
use crate::ai::{ChatMessage, ToolCall, ToolSpec};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Streamed completion ending in `stop_reason: "tool_use"`; the
/// `tool_use.input` arrives split across four `input_json_delta`s.
const SSE_TOOLCALL: &str = include_str!("../../../fixtures/ai/anthropic/sse_toolcall.raw");
/// Streamed text completion ending in `stop_reason: "end_turn"`.
const SSE_TEXT: &str = include_str!("../../../fixtures/ai/anthropic/sse_text.raw");
/// Partial text deltas followed by a mid-stream `error` event.
const SSE_ERROR: &str = include_str!("../../../fixtures/ai/anthropic/sse_error.raw");
/// `GET /v1/models` listing (`{"data":[...]}`).
const MODELS: &str = include_str!("../../../fixtures/ai/anthropic/models.json");
/// `rate_limit_error` envelope; the test response carries the standard
/// `Retry-After` header.
const ERR_429_BODY: &str = include_str!("../../../fixtures/ai/anthropic/err_429_retry_after.json");
/// `overloaded_error` (HTTP 529) — typed `RateLimited`, same outcome
/// as 429.
const ERR_529: &str = include_str!("../../../fixtures/ai/anthropic/err_529_overloaded.json");
/// `authentication_error`.
const ERR_401: &str = include_str!("../../../fixtures/ai/anthropic/err_401.json");
/// `request_too_large` — the Anthropic context-length failure.
const ERR_CONTEXT_LENGTH: &str =
    include_str!("../../../fixtures/ai/anthropic/err_context_length.json");

fn client_for(server: &MockServer, model: &str) -> AnthropicClient {
    AnthropicClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: "sk-ant-test".to_string(),
        model: model.to_string(),
        enabled: true,
        protocol: crate::config::AiProtocol::Anthropic,
    })
    .unwrap()
}

fn request(model: Option<&str>) -> ChatRequest {
    ChatRequest {
        model: model.map(str::to_string),
        messages: vec![
            ChatMessage::system("test system prompt"),
            ChatMessage::user("test question"),
        ],
        ..ChatRequest::default()
    }
}

async fn collect(rx: mpsc::UnboundedReceiver<AiStreamEvent>) -> Vec<AiStreamEvent> {
    let mut rx = rx;
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

async fn mount_completion(server: &MockServer, body: &'static str) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.as_bytes(), "text/event-stream"))
        .mount(server)
        .await;
}

/// Acceptance: the `tool_use` block's `input` assembles across its
/// four `input_json_delta` chunks; `stop_reason: "tool_use"` maps to
/// the shared `"tool_calls"` finish vocabulary.
#[tokio::test]
async fn tool_use_blocks_assemble_across_input_json_deltas() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TOOLCALL).await;
    let client = client_for(&server, "test-anthropic-toolcall");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    let done = events.iter().find_map(|e| match e {
        AiStreamEvent::Done(c) => Some(c),
        _ => None,
    });
    let completion = done.expect("stream ended with Done");
    assert_eq!(completion.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(completion.tool_calls.len(), 1);
    let call = &completion.tool_calls[0];
    assert_eq!(call.id, "toolu_01A09q90qw90lq917835lq9");
    assert_eq!(call.name, "search_web");
    let args: serde_json::Value =
        serde_json::from_str(&call.arguments).expect("assembled input is valid JSON");
    assert_eq!(args["query"], "current weather in Tokyo right now");
    let usage = completion.usage.expect("stream carried usage");
    assert_eq!(usage.prompt_tokens, 102);
    assert_eq!(usage.completion_tokens, 68);
    assert_eq!(usage.total_tokens, 170);
}

/// The text stream: `text_delta`s then a `Done` with `stop` + usage.
#[tokio::test]
async fn text_completion_streams_deltas_then_done() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TEXT).await;
    let client = client_for(&server, "test-anthropic-text");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    let deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            AiStreamEvent::Delta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas.concat(), "hello world");
    let completion = match events.last() {
        Some(AiStreamEvent::Done(c)) => c,
        other => panic!("expected terminal Done, got {other:?}"),
    };
    assert_eq!(completion.content, "hello world");
    assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
    assert!(completion.tool_calls.is_empty());
    let usage = completion.usage.expect("stream carried usage");
    assert_eq!(usage.prompt_tokens, 16);
    assert_eq!(usage.completion_tokens, 75);
    assert_eq!(usage.total_tokens, 91);
}

/// A mid-stream `error` event surfaces the deltas emitted so far, then
/// an `Error` — `api_error` classifies to `Provider` (no `Retry-After`
/// header exists to consult).
#[tokio::test]
async fn mid_stream_error_event_surfaces_partial_deltas() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_ERROR).await;
    let client = client_for(&server, "test-anthropic-miderr");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    let deltas: String = events
        .iter()
        .filter_map(|e| match e {
            AiStreamEvent::Delta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        deltas,
        "Tokyo's weather is currently quite warm, with temperatures reaching about"
    );
    match events.last() {
        Some(AiStreamEvent::Error(AiError::Provider { status, message })) => {
            assert_eq!(*status, 500);
            assert!(
                message.contains("upstream connection terminated"),
                "{message}"
            );
        }
        other => panic!("expected terminal Provider error, got {other:?}"),
    }
}

/// A 429 `rate_limit_error` with the standard `Retry-After` header
/// surfaces `retry_after_s`.
#[tokio::test]
async fn rate_limit_surfaces_retry_after_s() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "30")
                .set_body_string(ERR_429_BODY),
        )
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-429");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    match events.as_slice() {
        [AiStreamEvent::Error(AiError::RateLimited { retry_after_s })] => {
            assert_eq!(*retry_after_s, Some(30));
        }
        other => panic!("expected one RateLimited error, got {other:?}"),
    }
}

/// HTTP 529 `overloaded_error` — the canonical "back off" shape — is
/// the same `RateLimited` outcome.
#[tokio::test]
async fn overloaded_error_is_typed_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_string(ERR_529))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-529");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    match events.as_slice() {
        [AiStreamEvent::Error(AiError::RateLimited { retry_after_s })] => {
            assert_eq!(*retry_after_s, None);
        }
        other => panic!("expected one RateLimited error, got {other:?}"),
    }
}

#[tokio::test]
async fn auth_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string(ERR_401))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-401");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    match events.as_slice() {
        [AiStreamEvent::Error(AiError::Auth(message))] => {
            assert!(message.contains("x-api-key"), "{message}");
        }
        other => panic!("expected one Auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn context_length_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(413).set_body_string(ERR_CONTEXT_LENGTH))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-ctx");

    let rx = client
        .chat_stream(
            &request(None),
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let events = collect(rx).await;

    match events.as_slice() {
        [AiStreamEvent::Error(AiError::ContextLength(message))] => {
            assert!(message.contains("prompt is too long"), "{message}");
        }
        other => panic!("expected one ContextLength error, got {other:?}"),
    }
}

/// `GET /v1/models` parses the `{"data": [...]}` listing and the
/// second call inside the TTL is served from cache (one upstream
/// request total).
#[tokio::test]
async fn models_listing_parses_and_caches() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-models");

    let first = client.models(Duration::from_secs(10)).await.unwrap();
    assert!(
        first.iter().any(|m| m.id == "claude-sonnet-4-20250514"),
        "fixture models parsed: {first:?}"
    );
    let second = client.models(Duration::from_secs(10)).await.unwrap();
    assert_eq!(first.len(), second.len());

    let seen = server.received_requests().await.unwrap();
    assert_eq!(
        seen.len(),
        1,
        "cached /v1/models hit the wire again: {seen:?}"
    );
}

/// TTL expiry refetches.
#[tokio::test]
async fn models_cache_expires() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-anthropic-models-ttl").with_models_ttl(Duration::ZERO);

    client.models(Duration::from_secs(10)).await.unwrap();
    client.models(Duration::from_secs(10)).await.unwrap();

    let seen = server.received_requests().await.unwrap();
    assert_eq!(seen.len(), 2, "expired cache must refetch");
}

/// The request is the Messages-API shape: `x-api-key` +
/// `anthropic-version` headers; `system` is a top-level field (no
/// `system` message); flat `{name, input_schema}` tools;
/// `{"type":"auto"}` tool_choice; `max_tokens` always sent.
#[tokio::test]
async fn request_body_is_anthropic_shape() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TEXT).await;
    let client = client_for(&server, "test-anthropic-wire");

    let rx = client
        .chat_stream(
            &ChatRequest {
                model: Some("test-anthropic-wire".to_string()),
                messages: vec![
                    ChatMessage::system("test system prompt"),
                    ChatMessage::user("test question"),
                ],
                tools: vec![ToolSpec {
                    name: "search_web".to_string(),
                    description: "Search the web".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                }],
                tool_choice: Some(serde_json::json!("auto")),
                max_tokens: Some(64),
                temperature: Some(0.2),
            },
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let _ = collect(rx).await;

    let seen = server.received_requests().await.unwrap();
    assert_eq!(seen.len(), 1);
    let req = &seen[0];
    assert_eq!(
        req.headers.get("x-api-key").and_then(|v| v.to_str().ok()),
        Some("sk-ant-test")
    );
    assert_eq!(
        req.headers
            .get("anthropic-version")
            .and_then(|v| v.to_str().ok()),
        Some("2023-06-01")
    );
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["model"], "test-anthropic-wire");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 64);
    assert_eq!(body["system"], "test system prompt");
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    assert_eq!(body["messages"][0]["content"][0]["text"], "test question");
    assert_eq!(body["tools"][0]["name"], "search_web");
    assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(body["tool_choice"], serde_json::json!({"type": "auto"}));
    assert_eq!(body["temperature"], 0.2);
}

/// Assistant `tool_calls` echo back as `tool_use` blocks; `tool`
/// results ride inside the following `user` message as `tool_result`
/// blocks — and `max_tokens` defaults when `ChatRequest` omits it.
#[tokio::test]
async fn assistant_tool_use_and_tool_result_round_trip() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TEXT).await;
    let client = client_for(&server, "test-anthropic-echo");

    let rx = client
        .chat_stream(
            &ChatRequest {
                messages: vec![
                    ChatMessage::user("weather in tokyo?"),
                    ChatMessage::assistant_tool_calls(vec![ToolCall {
                        id: "toolu_01A09q90qw90lq917835lq9".to_string(),
                        name: "search_web".to_string(),
                        arguments: "{\"query\": \"tokyo weather\"}".to_string(),
                    }]),
                    ChatMessage::tool_result("toolu_01A09q90qw90lq917835lq9", "Tokyo: 22C, clear"),
                ],
                ..ChatRequest::default()
            },
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    let _ = collect(rx).await;

    let seen = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
    assert!(
        body["system"].is_null(),
        "no system messages => no system field: {body}"
    );
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"][0]["text"], "weather in tokyo?");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["type"], "tool_use");
    assert_eq!(
        messages[1]["content"][0]["id"],
        "toolu_01A09q90qw90lq917835lq9"
    );
    assert_eq!(messages[1]["content"][0]["name"], "search_web");
    assert_eq!(
        messages[1]["content"][0]["input"],
        serde_json::json!({"query": "tokyo weather"})
    );
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(
        messages[2]["content"][0]["tool_use_id"],
        "toolu_01A09q90qw90lq917835lq9"
    );
    assert_eq!(messages[2]["content"][0]["content"], "Tokyo: 22C, clear");
}

#[tokio::test]
async fn empty_model_and_messages_rejected() {
    let server = MockServer::start().await;
    let client = AnthropicClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: String::new(),
        model: String::new(),
        enabled: true,
        protocol: crate::config::AiProtocol::Anthropic,
    })
    .unwrap();
    assert!(matches!(
        client.chat_stream(&request(None), Duration::from_secs(1), AiCallCtx::default()),
        Err(AiError::Parse(_))
    ));
    let client = client_for(&server, "test-model");
    assert!(matches!(
        client.chat_stream(
            &ChatRequest::default(),
            Duration::from_secs(1),
            AiCallCtx::default()
        ),
        Err(AiError::Parse(_))
    ));
}
