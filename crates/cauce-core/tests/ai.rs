//! W4-01: one `audit` row per provider call — model, tokens, ms,
//! request_id; never the prompt text.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use cauce_core::{
    AiCallCtx, AiConfig, AiProtocol, AiStreamEvent, AnthropicClient, ChatMessage, ChatRequest,
    OpenAiClient,
};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Only `StubStore` is used here; the module compiles per test binary.
#[allow(dead_code)]
mod support;
use support::*;

/// Real streamed text completion (fixture: `finish_reason: "stop"`,
/// usage 16/75/91).
const SSE_TEXT: &str = include_str!("../fixtures/ai/sse_text.raw");

#[tokio::test]
async fn provider_call_writes_audit_row() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(SSE_TEXT.as_bytes(), "text/event-stream"),
        )
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let model = "test-openai-audit";
    let client = OpenAiClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: model.to_string(),
        enabled: true,
        protocol: AiProtocol::OpenAi,
    })
    .unwrap()
    .with_audit(store.clone() as Arc<dyn cauce_core::Store>);

    let request_id = Uuid::now_v7();
    let prompt = "the prompt text must never be audited";
    let mut rx = client
        .chat_stream(
            &ChatRequest {
                messages: vec![ChatMessage::user(prompt)],
                ..ChatRequest::default()
            },
            Duration::from_secs(10),
            AiCallCtx {
                actor: Some("api".to_string()),
                request_id: Some(request_id),
            },
        )
        .unwrap();
    while let Some(event) = rx.recv().await {
        if let AiStreamEvent::Error(e) = event {
            panic!("provider call failed: {e}");
        }
    }

    let audits = store.audits.lock().unwrap();
    assert_eq!(audits.len(), 1, "one provider call = one row: {audits:?}");
    let row = &audits[0];
    assert_eq!(row.actor, "api");
    assert_eq!(row.action, "ai.provider_call");
    assert_eq!(row.target, model);
    assert_eq!(row.request_id, Some(request_id));
    assert_eq!(row.details["tokens"]["prompt"], 16);
    assert_eq!(row.details["tokens"]["completion"], 75);
    assert_eq!(row.details["tokens"]["total"], 91);
    assert_eq!(row.details["outcome"], "ok");
    assert!(row.details["ms"].is_number(), "ms recorded: {row:?}");
    // The prompt never leaves the request body.
    let serialized = serde_json::to_string(row).unwrap();
    assert!(
        !serialized.contains(prompt),
        "audit row must not carry prompt text: {serialized}"
    );
}

#[tokio::test]
async fn failed_provider_call_still_writes_audit_row() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "7")
                .set_body_string(include_str!("../fixtures/ai/err_429_retry_after.json")),
        )
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let client = OpenAiClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: "test-openai-audit-429".to_string(),
        enabled: true,
        protocol: AiProtocol::OpenAi,
    })
    .unwrap()
    .with_audit(store.clone() as Arc<dyn cauce_core::Store>);

    let mut rx = client
        .chat_stream(
            &ChatRequest {
                messages: vec![ChatMessage::user("hi")],
                ..ChatRequest::default()
            },
            Duration::from_secs(10),
            AiCallCtx::default(),
        )
        .unwrap();
    while let Some(event) = rx.recv().await {
        match event {
            AiStreamEvent::Error(cauce_core::AiError::RateLimited { retry_after_s }) => {
                assert_eq!(retry_after_s, Some(7));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    let audits = store.audits.lock().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].actor, "ai", "no inbound actor -> default");
    assert_eq!(audits[0].details["outcome"], "rate_limited");
}

/// W4-05: the Anthropic client writes the same `ai.provider_call` row
/// (shared pump bookkeeping) — model, tokens, ms, request_id; never the
/// prompt text.
const SSE_ANTHROPIC_TEXT: &str = include_str!("../fixtures/ai/anthropic/sse_text.raw");

#[tokio::test]
async fn anthropic_provider_call_writes_audit_row() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(SSE_ANTHROPIC_TEXT.as_bytes(), "text/event-stream"),
        )
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let model = "test-anthropic-audit";
    let client = AnthropicClient::new(&AiConfig {
        base_url: server.uri(),
        api_key: "sk-ant-test".to_string(),
        model: model.to_string(),
        enabled: true,
        protocol: AiProtocol::Anthropic,
    })
    .unwrap()
    .with_audit(store.clone() as Arc<dyn cauce_core::Store>);

    let request_id = Uuid::now_v7();
    let prompt = "the prompt text must never be audited";
    let mut rx = client
        .chat_stream(
            &ChatRequest {
                messages: vec![ChatMessage::user(prompt)],
                ..ChatRequest::default()
            },
            Duration::from_secs(10),
            AiCallCtx {
                actor: Some("api".to_string()),
                request_id: Some(request_id),
            },
        )
        .unwrap();
    while let Some(event) = rx.recv().await {
        if let AiStreamEvent::Error(e) = event {
            panic!("provider call failed: {e}");
        }
    }

    let audits = store.audits.lock().unwrap();
    assert_eq!(audits.len(), 1, "one provider call = one row: {audits:?}");
    let row = &audits[0];
    assert_eq!(row.actor, "api");
    assert_eq!(row.action, "ai.provider_call");
    assert_eq!(row.target, model);
    assert_eq!(row.request_id, Some(request_id));
    assert_eq!(row.details["tokens"]["prompt"], 16);
    assert_eq!(row.details["tokens"]["completion"], 75);
    assert_eq!(row.details["tokens"]["total"], 91);
    assert_eq!(row.details["outcome"], "ok");
    let serialized = serde_json::to_string(row).unwrap();
    assert!(
        !serialized.contains(prompt),
        "audit row must not carry prompt text: {serialized}"
    );
}
