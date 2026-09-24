//! `GET /api/search/stream` SSE surface tests (W2-01): the batch/meta
//! frame order, dedupe keys, the `client` param fallback, and the two
//! error paths (pre-stream 400, mid-flight `error` event).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use cauce_engines::ReplayOpts;
use cauce_server::build_router;
use serde_json::{Value, json};
use tower::ServiceExt;

mod support;
use support::*;

#[tokio::test]
async fn search_stream_sends_result_batches_then_flattened_meta() {
    let (router, _state, _tmp) = app();
    let request = Request::builder()
        .method("GET")
        .uri("/api/search/stream?q=sse-test")
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("event: results\n"), "{body}");
    assert!(body.contains("event: meta\n"), "{body}");
    assert!(
        body.find("event: results").unwrap() < body.find("event: meta").unwrap(),
        "results must precede terminal meta: {body}"
    );
    let meta_frame = body
        .split("\n\n")
        .find(|frame| frame.starts_with("event: meta"))
        .expect("meta frame");
    let meta_json: Value = serde_json::from_str(
        meta_frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("meta data line"),
    )
    .unwrap();
    assert_eq!(meta_json["request_id"], request_id);
    assert_eq!(meta_json["engines_skipped"], json!([]));
    assert_eq!(meta_json["order"].as_array().unwrap().len(), 10);
}

/// A rejected engine pin is a real 400 before the stream opens (matching
/// `/api/search`), not a 200 carrying an `error` event.
#[tokio::test]
async fn search_stream_unknown_engine_pin_is_400() {
    let (router, _state, _tmp) = app();
    let request = Request::builder()
        .method("GET")
        .uri("/api/search/stream?q=sse-error&engines=unknown")
        .body(Body::empty())
        .unwrap();
    let (status, _headers, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "unknown_engines");
}

/// Failures past the pin check still arrive as a terminal `error` event
/// on the open stream (here: an all-failed fan-out from a blocked engine).
#[tokio::test]
async fn search_stream_maps_mid_flight_failures_to_error_events() {
    let (state, _tmp) = test_state_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });
    let router = build_router(state);
    let request = Request::builder()
        .method("GET")
        .uri("/api/search/stream?q=sse-error")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("event: error\n"), "{body}");
    assert!(!body.contains("event: meta\n"), "{body}");
    assert!(body.contains("\"code\":\"upstream_failed\""), "{body}");
}

/// Every streamed result carries its server-side dedupe `key`
/// (`normalize_url` of `url`), and `meta.order` lists the same keys, so
/// the page dedupes on the form the merge used.
#[tokio::test]
async fn search_stream_results_carry_dedupe_keys_matching_meta_order() {
    let (router, _state, _tmp) = app();
    let request = Request::builder()
        .method("GET")
        .uri("/api/search/stream?q=sse-keys")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    let mut keys = Vec::new();
    let mut order = Vec::new();
    for frame in body.split("\n\n") {
        let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) else {
            continue;
        };
        let payload: Value = serde_json::from_str(data).unwrap();
        if frame.starts_with("event: results") {
            for result in payload["results"].as_array().unwrap() {
                let url = result["url"].as_str().unwrap();
                let key = result["key"]
                    .as_str()
                    .unwrap_or_else(|| panic!("streamed result missing dedupe key: {result}"));
                assert_eq!(
                    key,
                    cauce_core::normalize_url(&url.parse().unwrap()).as_str(),
                    "key must be the normalized form of url"
                );
                keys.push(key.to_string());
            }
        }
        if frame.starts_with("event: meta") {
            order = payload["order"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
        }
    }
    keys.sort();
    order.sort();
    assert_eq!(keys, order, "meta.order must list the emitted dedupe keys");
}

/// `client=ui|api|mcp` is a `X-Cauce-Client` fallback for clients that
/// cannot set headers (EventSource): the log row takes the param's kind
/// when the header is absent, an unknown value is ignored, and the header
/// always wins.
#[tokio::test]
async fn search_stream_client_param_fills_client_kind_when_header_absent() {
    let (router, state, _tmp) = app();
    for uri in [
        "/api/search/stream?q=client-ui&client=ui",
        "/api/search/stream?q=client-bogus&client=bogus",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        to_bytes(response.into_body(), usize::MAX).await.unwrap();
    }
    let request = Request::builder()
        .method("GET")
        .uri("/api/search/stream?q=client-header&client=ui")
        .header("x-cauce-client", "cli")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), usize::MAX).await.unwrap();

    let filter = cauce_core::HistoryFilter::default();
    let rows = state.store().list_history(&filter).await.unwrap();
    let client_of = |q: &str| {
        rows.iter()
            .find_map(|row| match row {
                cauce_core::HistoryItem::Search(s) if s.query == q => Some(s.client.label()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no search_log row for {q}"))
    };
    assert_eq!(client_of("client-ui"), "ui");
    assert_eq!(client_of("client-bogus"), "api");
    assert_eq!(client_of("client-header"), "cli");
}
