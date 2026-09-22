//! HTMX search page integration tests (W0-10).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use oxe_core::SearchPipeline;
use oxe_core::config::Config;
use oxe_core::{
    CacheKey, ClientKind, EngineId, HistoryFilter, SafeSearch, SearchRequest, SearchResponse,
    StoreTuning,
};
use oxe_engines::{Replay, ReplayOpts};
use oxe_server::{AppState, build_router};
use oxe_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, Config::default());
    (state, tmp)
}

fn app() -> (Router, AppState, tempfile::TempDir) {
    let (state, tmp) = test_state();
    (build_router(state.clone()), state, tmp)
}

fn req_html(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Accept", "text/html")
        .body(Body::empty())
        .unwrap()
}

async fn call_html(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    call_html(router, req_html(Method::GET, uri)).await
}

async fn get_json(router: &Router, uri: &str) -> (StatusCode, SearchResponse) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Accept", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: SearchResponse = serde_json::from_slice(&bytes).expect("valid JSON");
    (status, body)
}

fn expected_badge(resp: &SearchResponse) -> String {
    match &resp.meta.source {
        oxe_core::Source::Cache { age_s, ttl_s, .. } => {
            format!("cached · {age_s} s ago · ttl {ttl_s} s")
        }
        oxe_core::Source::Network => {
            let engines = resp
                .meta
                .engines_used
                .iter()
                .filter(|r| matches!(r.status, oxe_core::EngineStatus::Ok))
                .map(|r| r.engine.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("live · {} ms · {engines}", resp.meta.elapsed_ms)
        }
    }
}

#[tokio::test]
async fn landing_page_has_search_form() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<form"), "landing page should contain a form");
    assert!(
        body.contains("action=\"/search\""),
        "form should post to /search"
    );
}

#[tokio::test]
async fn search_page_renders_replay_results_and_live_badge() {
    let (app, _state, _tmp) = app();
    // Use a fresh pinned query so the HTML request is the network hit.
    let q = "livebadgetest";
    let engines = "replay";
    let (status, body) = get_html(&app, &format!("/search?q={q}&engines={engines}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(&format!("{q}:")),
        "body should contain a replay title for query {q}"
    );
    assert!(
        body.contains("<img"),
        "result rows should include a favicon image"
    );
    assert!(
        body.contains("live ·"),
        "first search should show the live badge"
    );
    assert!(
        body.contains(" ms · replay"),
        "live badge should mention replay timing"
    );
    assert!(
        body.contains("https://icons.duckduckgo.com/ip3/"),
        "favicon src should use the DuckDuckGo icon service"
    );
}

#[tokio::test]
async fn second_search_is_cached() {
    let (app, _state, _tmp) = app();
    let q = "cachetest";
    let (_, _) = get_json(&app, &format!("/api/search?q={q}")).await;
    let (_, api_resp) = get_json(&app, &format!("/api/search?q={q}")).await;
    let expected = expected_badge(&api_resp);

    let (status, body) = get_html(&app, &format!("/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("cached"),
        "repeated query should show the cached badge"
    );
    assert!(body.contains("ttl"), "cached badge should mention the ttl");
    assert!(
        body.contains(&expected),
        "cached badge should exactly match response meta: {expected}"
    );
}

#[tokio::test]
async fn search_supports_json_accept() {
    let (app, _state, _tmp) = app();
    let request = Request::builder()
        .method(Method::GET)
        .uri("/search?q=x")
        .header("Accept", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert!(
        !body["results"].as_array().unwrap().is_empty(),
        "JSON should have results"
    );
    assert!(body["meta"]["source"].is_object() || body["meta"]["source"] == "network");
    assert!(
        body["meta"]["request_id"].as_str().is_some(),
        "JSON meta should include a request_id"
    );
}

#[tokio::test]
async fn htmx_request_returns_results_partial() {
    let (app, _state, _tmp) = app();
    let request = Request::builder()
        .method(Method::GET)
        .uri("/search?q=x")
        .header("Accept", "text/html")
        .header("HX-Request", "true")
        .body(Body::empty())
        .unwrap();
    let (status, body) = call_html(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.starts_with("<div id=\"results\""),
        "HTMX response should be the partial"
    );
    assert!(
        !body.contains("<!doctype html>"),
        "HTMX response should not be a full page"
    );
    assert!(body.contains("x:"), "partial should contain a result title");
}

#[tokio::test]
async fn result_link_wrapper_pattern() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/search?q=x").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"hx-post="/api/click""#),
        "wrapper should hx-post the click beacon"
    );
    assert!(
        body.contains(r#"hx-headers='{"X-Oxe-Client":"ui"}'"#),
        "wrapper should send X-Oxe-Client: ui"
    );
    // The beacon payload is JSON in hx-vals; form-flattening extensions like
    // json-enc stringify scalars, so `position` may arrive as "0" — ClickRow
    // tolerates both (covered by a core deserialization test).
    assert!(
        body.contains(r#"hx-vals="{"#) || body.contains("hx-vals=\"{&quot;"),
        "hx-vals should carry the JSON beacon payload"
    );
    assert!(
        body.contains(r#"target="_blank""#),
        "result link should open in a new tab"
    );
    // The anchor should not carry hx-* itself; the wrapper article does.
    let link_start = body.find("<a href=\"").expect("result link");
    let link_end = body[link_start..].find('>').expect("link close") + link_start;
    let link_tag = &body[link_start..link_end];
    assert!(
        !link_tag.contains("hx-post"),
        "anchor must not have hx-post (otherwise navigation is cancelled)"
    );
}

#[tokio::test]
async fn query_hash_matches_canonical_request() {
    let (app, _state, _tmp) = app();
    let q = "x";
    let engines = "replay";
    let req = SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: Some(vec![EngineId::from(engines)]),
        client: ClientKind::Ui,
    };
    let expected = CacheKey::from(&req).as_str().to_string();

    let (status, body) = get_html(&app, &format!("/search?q={q}&engines={engines}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(&expected),
        "hx-vals should contain the canonical CacheKey for the search: {expected}"
    );
}

#[tokio::test]
async fn more_button_preserves_query_params() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(
        &app,
        "/search?q=x&page=1&lang=en&time_range=day&safesearch=strict&engines=replay",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("lang=en"), "more url should preserve lang");
    assert!(
        body.contains("time_range=day"),
        "more url should preserve time_range"
    );
    assert!(
        body.contains("safesearch=strict"),
        "more url should preserve safesearch"
    );
    assert!(
        body.contains("engines=replay"),
        "more url should preserve engines"
    );
    assert!(body.contains("page=2"), "more url should request page 2");
}

#[tokio::test]
async fn click_beacon_records_ui_client() {
    let (app, state, _tmp) = app();
    let body = json!({
        "url": "https://example.com/replay-result",
        "position": 3,
    });
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/click")
        .header("Content-Type", "application/json")
        .header("X-Oxe-Client", "ui")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let history = state
        .store()
        .list_history(&HistoryFilter::default())
        .await
        .expect("history");
    let clicked = history.iter().any(|h| matches!(h, oxe_core::HistoryItem::Click(c) if c.url.as_str() == "https://example.com/replay-result" && c.position == 3 && c.client == ClientKind::Ui));
    assert!(clicked, "click should be persisted with client=ui");
}

/// The real beacon body as the browser sends it: `hx-vals` passes through
/// htmx's parameter flattening (all values become strings) before `json-enc`
/// re-serializes, so `position` arrives as `"3"` not `3`. Regression test —
/// this exact shape previously got a 400 from the wire.
#[tokio::test]
async fn click_beacon_accepts_stringified_scalars() {
    let (app, state, _tmp) = app();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/click")
        .header("Content-Type", "application/json")
        .header("X-Oxe-Client", "ui")
        .body(Body::from(
            r#"{"position":"3","url":"https://example.com/stringified"}"#,
        ))
        .unwrap();
    let resp = app.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let history = state
        .store()
        .list_history(&HistoryFilter::default())
        .await
        .expect("history");
    let clicked = history
        .iter()
        .any(|h| matches!(h, oxe_core::HistoryItem::Click(c) if c.position == 3));
    assert!(clicked, "stringified position should deserialize to 3");
}
