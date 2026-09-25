//! `POST /api/answer` and `GET /answer` tests (W4-03).
//!
//! The SSE route serializes each `AnswerFrame` as a named event (`step` /
//! `delta` / `sources` / `done` / `error`) over wiremock-replayed
//! provider transcripts; the page test asserts the stream shell, its
//! POST-SSE machinery, the `ungrounded` notice carrier, and the `/search`
//! `ask` link gating on `ai`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use cauce_core::{AiConfig, Config};
use cauce_server::build_router;
use serde_json::Value;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(dead_code)]
mod support;
use support::*;

const MODEL: &str = "test-answer-loop";

const SSE_TOOLCALL: &str = include_str!("../../cauce-core/fixtures/ai/sse_toolcall.raw");
const SSE_TOOLCALL_2: &str = include_str!("../../cauce-core/fixtures/ai/sse_toolcall_2.raw");
const SSE_ANSWER: &str = include_str!("../../cauce-core/fixtures/ai/sse_answer.raw");
const SSE_NOTOOLS: &str = include_str!("../../cauce-core/fixtures/ai/sse_notools.raw");
const SSE_ERROR: &str = include_str!("../../cauce-core/fixtures/ai/sse_error.raw");

/// An `AppState` with `ai.enabled` and the provider pointed at `server`:
/// `build_answer_loop` constructs the `OpenAiClient` exactly like boot.
async fn ai_app() -> (
    Router,
    cauce_server::AppState,
    tempfile::TempDir,
    MockServer,
) {
    let server = MockServer::start().await;
    let mut config = Config::default();
    config.ai = AiConfig {
        base_url: server.uri(),
        api_key: "sk-test".to_string(),
        model: MODEL.to_string(),
        enabled: true,
    };
    let (state, tmp) = test_state_with_config(config);
    (build_router(state.clone()), state, tmp, server)
}

/// wiremock matches in mount order and skips mocks past `up_to_n_times`,
/// so mounts run in call order (same convention as cauce-core's tests).
async fn mount_sse(server: &MockServer, body: &'static str, times: u64) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

/// `POST /api/answer` with a JSON body: status + headers + raw body text.
async fn post_answer(router: &Router, body: &str) -> (StatusCode, HeaderMap, String) {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/answer")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// Parse an SSE body into `(event, json)` pairs; keep-alive comments and
/// data-less frames drop out.
fn sse_events(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .filter_map(|raw| {
            let mut name = "message".to_string();
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
        .collect()
}

fn event_names(events: &[(String, Value)]) -> Vec<&str> {
    events.iter().map(|(n, _)| n.as_str()).collect()
}

/// The named-event order the wire contract pins: step(s) → delta(s) →
/// sources → done, with the answer text arriving before `done`.
#[tokio::test]
async fn answer_streams_frames_as_named_events() {
    let (router, _state, _tmp, server) = ai_app().await;
    mount_sse(&server, SSE_TOOLCALL, 1).await;
    mount_sse(&server, SSE_TOOLCALL_2, 1).await;
    mount_sse(&server, SSE_ANSWER, 1).await;

    let (status, headers, body) =
        post_answer(&router, r#"{"q":"current weather in Tokyo right now"}"#).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers["content-type"], "text/event-stream");

    let events = sse_events(&body);
    let names = event_names(&events);
    let step = names
        .iter()
        .position(|n| *n == "step")
        .expect("step frames");
    let delta = names
        .iter()
        .position(|n| *n == "delta")
        .expect("delta frames");
    let sources = names
        .iter()
        .position(|n| *n == "sources")
        .expect("sources frame");
    let done = names.iter().position(|n| *n == "done").expect("done frame");
    assert!(
        step < delta && delta < sources && sources < done,
        "step → delta → sources → done order: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|n| **n == "step").count(),
        2,
        "two search_web tool calls = two step frames: {names:?}"
    );

    let text: String = events
        .iter()
        .filter(|(n, _)| n == "delta")
        .map(|(_, v)| v["text"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        text.contains("22°C"),
        "answer text arrives before done: {text:?}"
    );

    let done_payload = &events[done].1;
    assert_eq!(done_payload["type"], "done");
    assert_eq!(done_payload["cached"], false);
    assert_eq!(done_payload["model"], "liquid/lfm-2.5-2.6b:free");
    assert!(
        done_payload.get("ungrounded").is_none() || done_payload["ungrounded"] == false,
        "a grounded answer is not ungrounded: {done_payload}"
    );
    assert_eq!(
        done_payload["request_id"].as_str().unwrap(),
        headers["x-request-id"].to_str().unwrap(),
        "done.request_id is the middleware request id"
    );
}

/// Acceptance 1: the page renders the stream shell wired to
/// `POST /api/answer` and the transcript's text lands before `done`
/// (proved on the wire by `answer_streams_frames_as_named_events`'s
/// order assertion).
#[cfg(feature = "ui")]
#[tokio::test]
async fn answer_page_renders_stream_shell() {
    let (router, _state, _tmp, _server) = ai_app().await;
    let (status, body) = get_html(&router, "/answer?q=what+is+rust").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for marker in [
        r#"id="answer-stream""#,
        r#"id="answer-steps""#,
        r#"id="answer-text""#,
        r#"id="answer-sources""#,
        r#"id="answer-related""#,
        r#"id="answer-error""#,
        "/api/answer",
        "text/event-stream",
        r#"var Q = "what is rust";"#,
        "X-Cauce-Client",
    ] {
        assert!(body.contains(marker), "answer shell missing {marker}");
    }
    assert!(!body.contains("ai-disabled"), "enabled build has no notice");
}

/// Acceptance 2: the no-tool transcript's `done` carries `ungrounded`
/// and the page ships the notice element the JS reveals on that flag.
#[cfg(feature = "ui")]
#[tokio::test]
async fn answer_page_renders_ungrounded_notice() {
    let (router, _state, _tmp, server) = ai_app().await;
    mount_sse(&server, SSE_NOTOOLS, 1).await;

    let (status, page) = get_html(&router, "/answer?q=what+is+rust").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains(r#"id="answer-ungrounded""#),
        "the ungrounded notice element must render: {page}"
    );
    assert!(
        page.contains("ungrounded"),
        "the notice copy is in the page"
    );

    let (status, _, body) = post_answer(&router, r#"{"q":"what is rust"}"#).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let events = sse_events(&body);
    let done = events
        .iter()
        .find(|(n, _)| n == "done")
        .map(|(_, v)| v)
        .expect("terminal done frame");
    assert_eq!(done["ungrounded"], true, "{done}");
    assert_eq!(done["confidence"], 6);
    assert_eq!(
        done["related_questions"].as_array().unwrap().len(),
        2,
        "related questions flow through to the page"
    );
    // Ungrounded answers carry no sources frame payload and are never
    // cached (core-enforced; the route must not add one either).
    assert_eq!(done["cached"], false);
    let sources = events.iter().find(|(n, _)| n == "sources").map(|(_, v)| v);
    assert_eq!(
        sources.expect("sources frame precedes done")["sources"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "a no-tool transcript streams zero sources"
    );
}

/// A mid-stream provider error rides the wire as a terminal `error`
/// event after the partial deltas — the page renders it inline.
#[tokio::test]
async fn answer_stream_error_is_a_terminal_event() {
    let (router, _state, _tmp, server) = ai_app().await;
    mount_sse(&server, SSE_ERROR, 1).await;

    let (status, _, body) = post_answer(&router, r#"{"q":"tokyo weather?"}"#).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let events = sse_events(&body);
    let names = event_names(&events);
    assert!(
        names.contains(&"delta"),
        "partial text still arrives: {names:?}"
    );
    let last = events.last().expect("terminal frame");
    assert_eq!(last.0, "error", "{names:?}");
    assert!(
        last.1["message"]
            .as_str()
            .unwrap()
            .contains("upstream connection terminated"),
        "{}",
        last.1["message"]
    );
}

/// Disabled `[ai]` answers 503 `ai_disabled` — the page's fetch sees the
/// same envelope and renders its message inline.
#[tokio::test]
async fn answer_route_disabled_is_503() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = post_answer(&router, r#"{"q":"x"}"#).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let env: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["error"]["code"], "ai_disabled");
}

/// Bad bodies reject before the stream opens: unparsable JSON, a blank
/// `q`, and unknown fields are all 400 `bad_request`.
#[tokio::test]
async fn answer_route_rejects_bad_bodies() {
    let (router, _state, _tmp, _server) = ai_app().await;
    for (body, why) in [
        ("{", "unparsable JSON"),
        (r#"{"q":"  "}"#, "blank q"),
        (r#"{"q":"x","extra":1}"#, "unknown field"),
        (r#"{"other":"x"}"#, "missing q"),
    ] {
        let (status, _, text) = post_answer(&router, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {text}");
        let env: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(env["error"]["code"], "bad_request", "{why}");
    }
}

/// `ai.enabled=false` renders the disabled notice with the `/settings`
/// link and no stream shell; the bare form page always renders.
#[cfg(feature = "ui")]
#[tokio::test]
async fn answer_page_disabled_notice() {
    let (router, _state, _tmp) = app();
    let (status, body) = get_html(&router, "/answer?q=x").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("AI mode is disabled"), "{body}");
    assert!(body.contains(r#"href="/settings""#), "{body}");
    assert!(
        !body.contains(r#"id="answer-stream""#),
        "no stream shell while disabled: {body}"
    );
}

/// Bare `/answer` (no `q`) is just the ask form — enabled or not.
#[cfg(feature = "ui")]
#[tokio::test]
async fn answer_page_bare_is_the_ask_form() {
    let (router, _state, _tmp, _server) = ai_app().await;
    let (status, body) = get_html(&router, "/answer").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(r#"id="answer-form""#), "{body}");
    assert!(
        !body.contains(r#"id="answer-stream""#),
        "no stream shell without a query: {body}"
    );
}

/// The `/search` meta line links `/answer?q=` only when an answer loop
/// exists — enabled and disabled builds both covered.
#[cfg(feature = "ui")]
#[tokio::test]
async fn search_page_ask_link_follows_ai() {
    let (router, _state, _tmp, _server) = ai_app().await;
    let (status, body) = get_html(&router, "/search?q=tokyo+weather").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains(r#"href="/answer?q=tokyo%20weather""#),
        "the ask link carries the encoded query: {body}"
    );

    let (router, _state, _tmp) = app();
    let (status, body) = get_html(&router, "/search?q=tokyo+weather").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !body.contains("/answer?q="),
        "no ask link while ai is disabled: {body}"
    );
}
