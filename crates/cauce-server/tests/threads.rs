//! `POST /api/answer` thread persistence + `/api/threads` surface tests
//! (UX-01): a thread upserted via `POST /api/answer` with `thread_id` is
//! readable by `GET /api/threads/{id}` and lists newest-first; another
//! owner's id is a 404 (never a leak); `DELETE` removes it; without
//! `thread_id` nothing persists and `done` carries no `thread_id`.
//!
//! Public-mode isolation: the owner key is the SHA-256 hex of the bearer
//! token (never the raw token), so two tokens never share a thread space.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::{AiConfig, Config};
use cauce_server::build_router;
use serde_json::Value;
use tower::ServiceExt;

mod support;
use support::*;

const SSE_NOTOOLS: &str = include_str!("../../cauce-core/fixtures/ai/sse_notools.raw");

/// An AI-enabled app in local mode (owner key `"local"`).
async fn ai_app() -> (Router, tempfile::TempDir, wiremock::MockServer) {
    let server = wiremock::MockServer::start().await;
    let mut config = Config::default();
    config.ai = AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: "test-threads-loop".to_string(),
        enabled: true,
        protocol: cauce_core::AiProtocol::OpenAi,
        ..AiConfig::default()
    };
    let (state, tmp) = test_state_with_config(config);
    (build_router(state.clone()), tmp, server)
}

fn auth_req(method: &str, uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

/// `POST /api/answer` with a JSON body; returns the parsed SSE events.
async fn post_answer(
    router: &Router,
    body: &str,
    token: Option<&str>,
) -> (StatusCode, Vec<(String, Value)>) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/answer")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let resp = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .expect("response");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let events = text
        .split("\n\n")
        .filter_map(|raw| {
            let mut name = String::new();
            let mut data = String::new();
            for line in raw.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    name = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data.push_str(rest.trim());
                }
            }
            if data.is_empty() {
                return None;
            }
            serde_json::from_str(&data).ok().map(|v| (name, v))
        })
        .collect();
    (status, events)
}

fn done_of(events: &[(String, Value)]) -> Value {
    events
        .iter()
        .find(|(n, _)| n == "done")
        .map(|(_, v)| v)
        .expect("terminal done frame")
        .clone()
}

/// With `thread_id`: the completed turn persists and `done.thread_id`
/// echoes the id; `GET /api/threads/{id}` returns the transcript
/// (history replay + this question/answer pair, strict user/assistant
/// alternation).
#[tokio::test]
async fn answer_with_thread_id_persists_and_done_echoes_it() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let id = uuid::Uuid::now_v7();
    let body = format!(r#"{{"q":"what is cauce","thread_id":"{id}"}}"#);
    let (status, events) = post_answer(&router, &body, None).await;
    assert_eq!(status, StatusCode::OK);
    let done = done_of(&events);
    assert_eq!(
        done["thread_id"].as_str().unwrap(),
        id.to_string(),
        "done echoes the persisted thread id: {done}"
    );

    let (status, row) = get_json(&router, &format!("/api/threads/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["id"].as_str().unwrap(), id.to_string());
    assert_eq!(row["owner"], "local");
    assert_eq!(
        row["title"], "what is cauce",
        "the question names the thread"
    );
    let turns = row["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2, "one question/answer pair: {row}");
    assert_eq!(turns[0]["role"], "user");
    assert_eq!(turns[0]["content"], "what is cauce");
    assert_eq!(turns[1]["role"], "assistant");
}

/// A follow-up turn (non-empty `history` + `thread_id`) appends: the
/// stored transcript is history plus the new pair.
#[tokio::test]
async fn followup_turn_appends_to_stored_thread() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let id = uuid::Uuid::now_v7();
    let first = format!(r#"{{"q":"what is cauce","thread_id":"{id}"}}"#);
    let (status, _) = post_answer(&router, &first, None).await;
    assert_eq!(status, StatusCode::OK);

    let second = format!(
        r#"{{"q":"and rust?","thread_id":"{id}","history":[{{"role":"user","content":"what is cauce"}},{{"role":"assistant","content":"a metasearch engine."}}]}}"#
    );
    let (status, events) = post_answer(&router, &second, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done_of(&events)["thread_id"], id.to_string());

    let (status, row) = get_json(&router, &format!("/api/threads/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let turns = row["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 4, "history replay plus the second pair: {row}");
    assert_eq!(turns[2]["role"], "user");
    assert_eq!(turns[2]["content"], "and rust?");
    assert_eq!(
        row["title"], "what is cauce",
        "title stays the first question"
    );
}

/// Without `thread_id` nothing persists and `done` carries no `thread_id`
/// (W7-04 behaviour bit-for-bit).
#[tokio::test]
async fn answer_without_thread_id_persists_nothing() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let (status, events) = post_answer(&router, r#"{"q":"what is cauce"}"#, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        done_of(&events).get("thread_id").is_none(),
        "no thread_id without persistence: {}",
        done_of(&events)
    );

    let (status, rows) = get_json(&router, "/api/threads").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        rows.as_array().unwrap().len(),
        0,
        "nothing was persisted: {rows}"
    );
}

/// An error stream persists nothing: a failed run must not extend the
/// stored transcript.
#[tokio::test]
async fn error_stream_persists_nothing() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let id = uuid::Uuid::now_v7();
    let body = format!(r#"{{"q":"what is cauce","thread_id":"{id}"}}"#);
    let (status, events) = post_answer(&router, &body, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        events.iter().any(|(n, _)| n == "error"),
        "the stream surfaces the failure"
    );

    let (status, _) = get_json(&router, &format!("/api/threads/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "nothing was persisted");
}

/// Listing is `updated_at` newest-first.
#[tokio::test]
async fn threads_list_newest_first() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let first = uuid::Uuid::now_v7();
    let (status, _) = post_answer(
        &router,
        &format!(r#"{{"q":"first question","thread_id":"{first}"}}"#),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Give the second thread a strictly later `updated_at`.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let second = uuid::Uuid::now_v7();
    let (status, _) = post_answer(
        &router,
        &format!(r#"{{"q":"second question","thread_id":"{second}"}}"#),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, rows) = get_json(&router, "/api/threads").await;
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"].as_str().unwrap(), second.to_string());
    assert_eq!(rows[1]["id"].as_str().unwrap(), first.to_string());
}

/// Public mode: two bearer identities never share a thread space, and
/// the stored owner key is a SHA-256 hex (64 chars), never the token.
#[tokio::test]
async fn public_mode_owner_isolation() {
    let server = wiremock::MockServer::start().await;
    let mut config = Config::default();
    config.server.public_instance = true;
    config.auth.admin_tokens = vec!["admin-token".to_string()];
    config.ai = AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: "test-threads-loop".to_string(),
        enabled: true,
        protocol: cauce_core::AiProtocol::OpenAi,
        ..AiConfig::default()
    };
    let (state, _tmp) = test_state_with_config(config);
    let router = build_router(state);
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let id = uuid::Uuid::now_v7();
    let body = format!(r#"{{"q":"secret question","thread_id":"{id}"}}"#);
    let (status, _) = post_answer(&router, &body, Some("alice-token")).await;
    assert_eq!(status, StatusCode::OK);

    // Alice reads her own thread.
    let (status, _row) = get_json(&router, &format!("/api/threads/{id}")).await;
    // The read also goes through the bearer identity, but `get_json`
    // carries no header — in public mode that is the anonymous owner,
    // so this must 404 rather than leak.
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Alice (the right bearer) reads it: owner is the token hash.
    let resp = router
        .clone()
        .oneshot(auth_req(
            "GET",
            &format!("/api/threads/{id}"),
            Some("alice-token"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let row: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(row["owner"].as_str().unwrap().len(), 64);
    assert_ne!(
        row["owner"], "alice-token",
        "the raw token is never the owner key"
    );

    // Bob gets a 404 on Alice's id.
    let resp = router
        .clone()
        .oneshot(auth_req(
            "GET",
            &format!("/api/threads/{id}"),
            Some("bob-token"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Alice's list shows her thread; Bob's list is empty.
    let alice = router
        .clone()
        .oneshot(auth_req("GET", "/api/threads", Some("alice-token")))
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(alice.into_body(), usize::MAX)
        .await
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);

    let bob = router
        .clone()
        .oneshot(auth_req("GET", "/api/threads", Some("bob-token")))
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(bob.into_body(), usize::MAX)
        .await
        .unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 0);
}

/// `DELETE /api/threads/{id}` removes the row; a second delete is a 404.
#[tokio::test]
async fn delete_removes_thread() {
    let (router, _tmp, server) = ai_app().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(SSE_NOTOOLS, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let id = uuid::Uuid::now_v7();
    let (status, _) = post_answer(
        &router,
        &format!(r#"{{"q":"what is cauce","thread_id":"{id}"}}"#),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _headers, body) =
        call_json(&router, req("DELETE", &format!("/api/threads/{id}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);

    let (status, _headers, _) =
        call_json(&router, req("DELETE", &format!("/api/threads/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = get_json(&router, &format!("/api/threads/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A malformed id is a `bad_request`, not a 404.
#[tokio::test]
async fn malformed_thread_id_is_bad_request() {
    let (router, _tmp, _server) = ai_app().await;
    let (status, body) = get_json(&router, "/api/threads/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "bad_request");
}
