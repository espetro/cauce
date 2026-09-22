//! HTMX search page integration tests (W0-10).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use oxe_core::config::Config;
use oxe_core::{HistoryFilter, SearchPipeline, StoreTuning};
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
    let (status, body) = get_html(&app, "/search?q=x").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("x:"),
        "body should contain a replay title for query x"
    );
    assert!(
        body.contains("<img"),
        "result rows should include a favicon image"
    );
    assert!(
        body.contains("live"),
        "first request should show the live badge"
    );
    assert!(
        body.contains("https://icons.duckduckgo.com/ip3/"),
        "favicon src should use the DuckDuckGo icon service"
    );
}

#[tokio::test]
async fn second_search_is_cached() {
    let (app, _state, _tmp) = app();
    let _ = get_html(&app, "/search?q=cachetest").await;
    let (status, body) = get_html(&app, "/search?q=cachetest").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("cached"),
        "repeated query should show the cached badge"
    );
    assert!(body.contains("ttl"), "cached badge should mention the ttl");
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
async fn more_button_requests_page_two() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/search?q=x&page=1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("hx-get=\""),
        "results should include an hx-get more button"
    );
    assert!(
        body.contains("page=2"),
        "more button should request the next page"
    );
}

#[tokio::test]
async fn click_beacon_is_persisted() {
    let (app, state, _tmp) = app();
    let body = json!({
        "url": "https://example.com/replay-result",
        "position": 3,
    });
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/click")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let history = state
        .store()
        .list_history(&HistoryFilter::default())
        .await
        .expect("history");
    let clicked = history.iter().any(|h| matches!(h, oxe_core::HistoryItem::Click(c) if c.url.as_str() == "https://example.com/replay-result" && c.position == 3));
    assert!(clicked, "click should be persisted in history");
}
