//! PUB-03: per-request `ai` overrides (BYOK) on `POST /api/answer` and
//! the `free_daily_answers` per-IP budget for admin-paid answers.
//!
//! The provider side is wiremock: an override that reaches the provider
//! lands on the mock with the caller's key/model, and gate rejections
//! never leave the process (zero recorded requests).

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, HeaderName, Method, Request, StatusCode};
use cauce_core::AiProtocol;
use cauce_core::config::{AiConfig, Config};
use cauce_server::build_router;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(dead_code)]
mod support;
use support::*;

const XFF: HeaderName = HeaderName::from_static("x-forwarded-for");
const SSE_ANSWER: &str = include_str!("../../cauce-core/fixtures/ai/sse_answer.raw");

/// A public instance whose `[ai]` talks to `server`, `rate_limit` off
/// (the budget tests run their own counter — the rps governor would
/// double-count) and BYOK gates caller-chosen.
async fn byok_app(
    mut ai: AiConfig,
    server: &MockServer,
    free_daily_answers: u32,
) -> (axum::Router, cauce_server::AppState, tempfile::TempDir) {
    ai.base_url = server.uri();
    ai.free_daily_answers = free_daily_answers;
    let mut cfg = Config::default();
    cfg.server.public_instance = true;
    cfg.auth.admin_tokens = vec!["test-admin-token".to_string()];
    cfg.rate_limit.enabled = Some(false);
    cfg.rate_limit.trust_proxy_headers = true;
    cfg.ai = ai;
    let (state, tmp) = test_state_with_config(cfg);
    (build_router(state.clone()), state, tmp)
}

/// The admin-paid `[ai]` configuration used across these tests.
fn admin_ai() -> AiConfig {
    AiConfig {
        api_key: "sk-admin".to_string(),
        model: "admin-model".to_string(),
        enabled: true,
        protocol: AiProtocol::OpenAi,
        ..AiConfig::default()
    }
}

async fn mount_sse(server: &MockServer, body: &'static str, times: u64) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

/// `POST /api/answer` as `ip` (via `x-forwarded-for`) with optional
/// `Authorization` header; returns status + headers + raw body.
async fn post_answer_as(
    router: &axum::Router,
    body: &str,
    ip: Option<&str>,
    auth: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/answer")
        .header("content-type", "application/json");
    if let Some(ip) = ip {
        req = req.header(&XFF, ip);
    }
    if let Some(auth) = auth {
        req = req.header("authorization", auth);
    }
    let resp = router
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn error_code(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body).unwrap_or_default()["error"]["code"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// The wiremock requests a server recorded, with `authorization` values
/// extracted for assertions.
async fn received_auth(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| {
            r.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

/// The caller's `api_key`/`model` reach the provider when
/// `allow_user_keys` is on — admin fields the override didn't name stay.
#[tokio::test]
async fn byok_override_reaches_provider() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 1).await;
    let mut ai = admin_ai();
    ai.allow_user_keys = true;
    let (router, _state, _tmp) = byok_app(ai, &server, 0).await;

    let (status, _h, body) = post_answer_as(
        &router,
        r#"{"q":"x","ai":{"api_key":"sk-user","model":"user-model"}}"#,
        Some("203.0.113.10"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let reqs = server.received_requests().await.expect("requests");
    assert_eq!(reqs.len(), 1);
    let auth = reqs[0]
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(auth, "Bearer sk-user", "BYOK key must reach the provider");
    let sent = String::from_utf8(reqs[0].body.clone()).unwrap();
    assert!(sent.contains("\"model\":\"user-model\""), "{sent}");
}

/// `ai` on the body while `allow_user_keys=false` → 4xx naming the gate
/// and nothing is forwarded to the provider.
#[tokio::test]
async fn byok_rejected_when_gate_off() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 1).await;
    let (router, _state, _tmp) = byok_app(admin_ai(), &server, 0).await;

    let (status, _h, body) = post_answer_as(
        &router,
        r#"{"q":"x","ai":{"api_key":"sk-user"}}"#,
        Some("203.0.113.11"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(error_code(&body), "forbidden");
    assert!(body.contains("allow_user_keys"), "{body}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "a rejected override must not reach the provider"
    );
}

/// `base_url` alone needs its own gate — even with `allow_user_keys`.
#[tokio::test]
async fn byok_base_url_rejected_when_gate_off() {
    let server = MockServer::start().await;
    let mut ai = admin_ai();
    ai.allow_user_keys = true;
    let (router, _state, _tmp) = byok_app(ai, &server, 0).await;

    let (status, _h, body) = post_answer_as(
        &router,
        &format!(
            r#"{{"q":"x","ai":{{"api_key":"sk-user","base_url":"{}"}}}}"#,
            server.uri()
        ),
        Some("203.0.113.12"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("allow_user_base_url"), "{body}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// A `base_url` override without a caller `api_key` blanks the admin
/// key rather than forwarding `Authorization` to a user-chosen host.
#[tokio::test]
async fn byok_base_url_never_leaks_admin_key() {
    let admin = MockServer::start().await;
    let caller = MockServer::start().await;
    mount_sse(&caller, SSE_ANSWER, 1).await;
    let mut ai = admin_ai();
    ai.allow_user_keys = true;
    ai.allow_user_base_url = true;
    let (router, _state, _tmp) = byok_app(ai, &admin, 0).await;

    let (status, _h, body) = post_answer_as(
        &router,
        &format!(r#"{{"q":"x","ai":{{"base_url":"{}"}}}}"#, caller.uri()),
        Some("203.0.113.13"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let caller_reqs = caller.received_requests().await.expect("requests");
    assert_eq!(caller_reqs.len(), 1);
    let auth = caller_reqs[0]
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        !auth.contains("sk-admin"),
        "admin key must not be forwarded to a caller-chosen base_url: {auth}"
    );
    assert!(
        admin
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// `free_daily_answers = 1`: one admin-paid answer per IP per UTC day;
/// the second 429s with `Retry-After`, BYOK skips the counter, other
/// IPs are unaffected, and the admin bearer is exempt.
#[tokio::test]
async fn free_daily_answers_budget() {
    let server = MockServer::start().await;
    // admin answer + BYOK answer + other-IP answer + admin-token answer.
    mount_sse(&server, SSE_ANSWER, 4).await;
    let mut ai = admin_ai();
    ai.allow_user_keys = true;
    let (router, _state, _tmp) = byok_app(ai, &server, 1).await;

    let (status, _h, body) =
        post_answer_as(&router, r#"{"q":"x"}"#, Some("203.0.113.1"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, headers, body) =
        post_answer_as(&router, r#"{"q":"x"}"#, Some("203.0.113.1"), None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(error_code(&body), "rate_limited");
    let wait: u64 = headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(
        (1..=86_400).contains(&wait),
        "Retry-After to UTC midnight: {wait}"
    );

    // BYOK never consumes the budget — the caller's provider pays.
    let (status, _h, body) = post_answer_as(
        &router,
        r#"{"q":"x","ai":{"api_key":"sk-user"}}"#,
        Some("203.0.113.1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // A different IP has its own day-bucket.
    let (status, _h, body) =
        post_answer_as(&router, r#"{"q":"x"}"#, Some("203.0.113.2"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The admin bearer is exempt regardless of budget.
    let (status, _h, body) = post_answer_as(
        &router,
        r#"{"q":"x"}"#,
        Some("203.0.113.1"),
        Some("Bearer test-admin-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// `free_daily_answers = 0` is unlimited — the old behaviour.
#[tokio::test]
async fn free_daily_answers_zero_is_unlimited() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 3).await;
    let (router, _state, _tmp) = byok_app(admin_ai(), &server, 0).await;
    for _ in 0..3 {
        let (status, _h, body) =
            post_answer_as(&router, r#"{"q":"x"}"#, Some("203.0.113.3"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
}

/// Precedence: `[ai]` unset + BYOK runs; `[ai]` set + BYOK wins over it.
#[tokio::test]
async fn byok_precedence() {
    // [ai] empty: the merged config turns on around the caller's key.
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 1).await;
    let ai = AiConfig {
        allow_user_keys: true,
        allow_user_base_url: true,
        ..AiConfig::default()
    };
    let (router, _state, _tmp) = byok_app(ai, &server, 0).await;
    let (status, _h, body) = post_answer_as(
        &router,
        &format!(
            r#"{{"q":"x","ai":{{"api_key":"sk-user","model":"m","base_url":"{}"}}}}"#,
            server.uri()
        ),
        Some("203.0.113.20"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // [ai] set: a full BYOK triple routes to the caller's endpoint and
    // key, not the admin's.
    let admin = MockServer::start().await;
    let caller = MockServer::start().await;
    mount_sse(&caller, SSE_ANSWER, 1).await;
    let mut ai = admin_ai();
    ai.allow_user_keys = true;
    ai.allow_user_base_url = true;
    let (router, _state, _tmp) = byok_app(ai, &admin, 0).await;
    let (status, _h, body) = post_answer_as(
        &router,
        &format!(
            r#"{{"q":"x","ai":{{"api_key":"sk-user","model":"user-model","base_url":"{}"}}}}"#,
            caller.uri()
        ),
        Some("203.0.113.21"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(received_auth(&caller).await, ["Bearer sk-user"]);
    assert!(
        admin
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// An override that merges down to a cred-less config lands on the
/// existing `ai_disabled` 503 rather than a BYOK-specific path.
#[tokio::test]
async fn byok_incomplete_override_is_disabled() {
    let server = MockServer::start().await;
    let ai = AiConfig {
        allow_user_keys: true,
        ..AiConfig::default()
    };
    let (router, _state, _tmp) = byok_app(ai, &server, 0).await;
    let (status, _h, body) = post_answer_as(
        &router,
        r#"{"q":"x","ai":{"model":"user-model"}}"#,
        Some("203.0.113.22"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_code(&body), "ai_disabled");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}
