//! Recorded fixtures replayed through `wiremock` — hermetic, no live
//! provider calls.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use super::*;
use crate::ai::{ChatMessage, ToolSpec};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Real streamed completion ending in `finish_reason: "tool_calls"`;
/// `function.arguments` arrives split across four delta chunks.
const SSE_TOOLCALL: &str = include_str!("../../../fixtures/ai/sse_toolcall.raw");
/// Real streamed text completion ending in `finish_reason: "stop"`.
const SSE_TEXT: &str = include_str!("../../../fixtures/ai/sse_text.raw");
/// Real `GET /models` (OpenAI `{"data":[...]}` format).
const MODELS: &str = include_str!("../../../fixtures/ai/models.json");
/// Real OpenRouter-style 429 error envelope (retry hint lives in
/// `error.metadata`, not an HTTP header).
const ERR_429_ENVELOPE: &str = include_str!("../../../fixtures/ai/err_429.json");
/// Hand-authored: OpenAI's 429 body shape; the test response carries
/// the standard `Retry-After` header.
const ERR_429_BODY: &str = include_str!("../../../fixtures/ai/err_429_retry_after.json");
/// Hand-authored OpenAI auth error.
const ERR_401: &str = include_str!("../../../fixtures/ai/err_401.json");
/// Hand-authored OpenAI context-length error.
const ERR_CONTEXT_LENGTH: &str = include_str!("../../../fixtures/ai/err_context_length.json");

fn client_for(server: &MockServer, model: &str) -> OpenAiClient {
    OpenAiClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: model.to_string(),
        enabled: true,
        protocol: crate::config::AiProtocol::OpenAi,
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
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.as_bytes(), "text/event-stream"))
        .mount(server)
        .await;
}

/// Acceptance: the recorded stream's `search_web` call assembles
/// across its four argument deltas.
#[tokio::test]
async fn tool_call_deltas_assemble_across_chunks() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TOOLCALL).await;
    let client = client_for(&server, "test-openai-toolcall");

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
    assert_eq!(call.id, "chatcmpl-tool-b0d6032a45b725d0");
    assert_eq!(call.name, "search_web");
    let args: serde_json::Value =
        serde_json::from_str(&call.arguments).expect("assembled arguments are valid JSON");
    assert_eq!(args["query"], "current weather in Tokyo right now");
    let usage = completion.usage.expect("stream carried usage");
    assert_eq!(usage.prompt_tokens, 102);
    assert_eq!(usage.completion_tokens, 68);
    assert_eq!(usage.total_tokens, 170);
}

/// The recorded text stream: content deltas then a `Done` with
/// `stop` + usage.
#[tokio::test]
async fn text_completion_streams_deltas_then_done() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TEXT).await;
    let client = client_for(&server, "test-openai-text");

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

/// Acceptance: a 429 with the standard `Retry-After` header surfaces
/// `retry_after_s`.
#[tokio::test]
async fn rate_limit_surfaces_retry_after_s() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "30")
                .set_body_string(ERR_429_BODY),
        )
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-429");

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

/// The recorded OpenRouter envelope carries the retry hint in
/// `error.metadata` (no `Retry-After` header) — it still surfaces.
#[tokio::test]
async fn rate_limit_reads_envelope_metadata() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string(ERR_429_ENVELOPE))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-429-meta");

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
            assert_eq!(*retry_after_s, Some(14));
        }
        other => panic!("expected one RateLimited error, got {other:?}"),
    }
}

#[tokio::test]
async fn auth_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string(ERR_401))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-401");

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
            assert!(message.contains("API key"), "{message}");
        }
        other => panic!("expected one Auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn context_length_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string(ERR_CONTEXT_LENGTH))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-ctx");

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
            assert!(message.contains("context length"), "{message}");
        }
        other => panic!("expected one ContextLength error, got {other:?}"),
    }
}

/// `GET /models` parses the OpenAI `{"data": [...]}` shape and the
/// second call inside the TTL is served from cache (one upstream
/// request total).
#[tokio::test]
async fn models_listing_parses_and_caches() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-models");

    let first = client.models(Duration::from_secs(10)).await.unwrap();
    assert!(
        first.iter().any(|m| m.id == "fireworks/ember-1"),
        "fixture models parsed"
    );
    let second = client.models(Duration::from_secs(10)).await.unwrap();
    assert_eq!(first.len(), second.len());

    let seen = server.received_requests().await.unwrap();
    assert_eq!(seen.len(), 1, "cached /models hit the wire again: {seen:?}");
}

/// TTL expiry refetches.
#[tokio::test]
async fn models_cache_expires() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
        .mount(&server)
        .await;
    let client = client_for(&server, "test-openai-models-ttl").with_models_ttl(Duration::ZERO);

    client.models(Duration::from_secs(10)).await.unwrap();
    client.models(Duration::from_secs(10)).await.unwrap();

    let seen = server.received_requests().await.unwrap();
    assert_eq!(seen.len(), 2, "expired cache must refetch");
}

/// The request body is the OpenAI shape: stream + include_usage,
/// auth header, no tools block when empty.
#[tokio::test]
async fn request_body_is_openai_shape() {
    let server = MockServer::start().await;
    mount_completion(&server, SSE_TEXT).await;
    let client = client_for(&server, "test-openai-wire");

    let rx = client
        .chat_stream(
            &ChatRequest {
                model: Some("test-openai-wire".to_string()),
                messages: vec![ChatMessage::user("hello")],
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
        req.headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        Some("Bearer sk-test")
    );
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["model"], "test-openai-wire");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "search_web");
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["max_tokens"], 64);
}

#[tokio::test]
async fn empty_model_and_messages_rejected() {
    let server = MockServer::start().await;
    let client = OpenAiClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: String::new(),
        model: String::new(),
        enabled: true,
        protocol: crate::config::AiProtocol::OpenAi,
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
