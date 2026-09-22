//! `/audit` and `/trace/{id}` page tests (W2-06).
//!
//! Acceptance: a cache delete made the way W2-04's page makes it
//! (`DELETE /api/cache/{key}` with `X-Cauce-Client: ui`) shows on `/audit`
//! with actor `ui`; `/trace/<id>` of a replay search lists the engine span
//! (rendered through the same `trace_request`/`render_trace` pair as
//! `cauce trace`).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::SearchPipeline;
use cauce_core::StoreTuning;
use cauce_core::config::{Config, EnvMap};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

/// A tempdir-backed state whose `Config` points `data_dir` (and therefore
/// `logs_dir`) inside the tempdir, so `/trace/{id}` reads fixture JSONL the
/// test writes — the same resolution `cauce serve` uses.
fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let env: EnvMap = BTreeMap::from([
        (
            "CAUCE_DATA_DIR".to_string(),
            tmp.path().join("data").to_string_lossy().into_owned(),
        ),
        (
            "CAUCE_CONFIG_DIR".to_string(),
            tmp.path().join("cfg").to_string_lossy().into_owned(),
        ),
    ]);
    let config = Config::from_raw(&toml::Value::Table(Default::default()), &env).expect("config");
    let state = AppState::new(pipeline, store, config);
    (state, tmp)
}

fn app() -> (Router, AppState, tempfile::TempDir) {
    let (state, tmp) = test_state();
    (build_router(state.clone()), state, tmp)
}

async fn call(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Accept", "text/html")
        .body(Body::empty())
        .unwrap();
    call(router, request).await
}

async fn get_json(router: &Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Accept", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).expect("valid JSON"))
}

/// Seed a cache row and delete it the way the W2-04 cache page does:
/// `DELETE /api/cache/{key}` carrying `X-Cauce-Client: ui`. Returns the
/// deleted key.
async fn ui_cache_delete(router: &Router, q: &str) -> String {
    let (status, _) = get_json(router, &format!("/api/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get_json(router, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    let key = body[0]["key"].as_str().expect("seeded cache entry");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/cache/{key}"))
        .header("X-Cauce-Client", "ui")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    key.to_string()
}

/// W2-06 acceptance, first half: a cache delete written by the UI surface
/// appears on `/audit` with actor `ui`, newest first, with expandable
/// details and a `/trace/<id>` link on its request id.
#[tokio::test]
async fn audit_page_lists_ui_cache_delete() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-seed").await;

    // A second audited action proves the newest-first ordering.
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/engines/replay/reset")
        .header("X-Cauce-Client", "ui")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(request).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);

    let (status, body) = get_html(&app, "/audit").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Action cells render `<code>…</code>`; a bare `contains("cache.delete")`
    // would match the filter form's placeholder text.
    assert!(
        body.contains("<code>cache.delete</code>"),
        "delete row missing: {body}"
    );
    assert!(body.contains("<td>ui</td>"), "actor ui missing: {body}");
    assert!(
        body.contains("<code>engine.reset</code>"),
        "reset row missing: {body}"
    );
    assert!(
        body.find("<code>engine.reset</code>") < body.find("<code>cache.delete</code>"),
        "rows should be newest first:\n{body}"
    );
    assert!(
        body.contains("<details"),
        "details should be expandable: {body}"
    );
    assert!(
        body.contains("href=\"/trace/"),
        "request_id should link to /trace/<id>: {body}"
    );
    // Every page shows the request id of the data it rendered.
    assert!(
        body.contains("class=\"request-id\""),
        "footer request id missing: {body}"
    );
}

/// `actor` and `action` URL params filter the listing; a filter that
/// matches nothing renders the filtered empty state, not an error.
#[tokio::test]
async fn audit_page_filters_by_actor_and_action() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-filter-seed").await;

    let (status, body) = get_html(&app, "/audit?actor=ui").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<code>cache.delete</code>"), "{body}");

    let (status, body) = get_html(&app, "/audit?actor=cli").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<code>cache.delete</code>"),
        "cli filter should hide the ui row: {body}"
    );
    assert!(
        body.contains("No audit rows match"),
        "filtered empty state missing: {body}"
    );

    let (status, body) = get_html(&app, "/audit?action=cache.delete").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<code>cache.delete</code>"), "{body}");

    let (status, body) = get_html(&app, "/audit?action=config.put").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<code>cache.delete</code>"), "{body}");

    // Unknown params are 400s, same as the JSON surface.
    let (status, _) = get_html(&app, "/audit?bogus=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// `Accept: application/json` on `/audit` delegates to the `/api/audit`
/// handler — one data path for page and API.
#[tokio::test]
async fn audit_page_negotiates_json() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-json-seed").await;

    let (status, body) = get_json(&app, "/audit").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("audit rows");
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["action"], "cache.delete");
    assert_eq!(rows[0]["actor"], "ui");
}

/// JSONL records in the shape `jsonl.rs` writes for a replay search: the
/// request span, the `engine` span (open + close with `busy_ms`) and one
/// event inside it.
fn fixture_lines(request_id: &uuid::Uuid) -> Vec<String> {
    let rid = request_id.to_string();
    vec![
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.000Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{rid}","span":{{"id":1,"name":"pipeline.search","parent":null,"fields":{{"request_id":"{rid}","query":"replay trace","client":"ui"}}}},"spans":["pipeline.search"]}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.010Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{rid}","span":{{"id":2,"name":"engine","parent":1,"fields":{{"request_id":"{rid}","engine":"replay","tier":1}}}},"spans":["pipeline.search","engine"]}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"event","ts":"2026-10-28T12:00:00.020Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{rid}","span":{{"id":2,"name":"engine"}},"spans":["pipeline.search","engine"],"fields":{{"message":"engine done","results":10}}}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"span_close","ts":"2026-10-28T12:00:00.030Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{rid}","span":{{"id":2,"name":"engine","parent":1,"fields":{{"request_id":"{rid}","engine":"replay","status":"ok","results":10}}}},"spans":["pipeline.search","engine"],"busy_ms":12.4}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"span_close","ts":"2026-10-28T12:00:00.050Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{rid}","span":{{"id":1,"name":"pipeline.search","parent":null,"fields":{{"request_id":"{rid}","query":"replay trace","client":"ui"}}}},"spans":["pipeline.search"],"busy_ms":43.1}}"#
        ),
    ]
}

/// W2-06 acceptance, second half: `/trace/<id>` of a replay search lists
/// the engine span. The page reads the same JSONL `cauce trace` reads and
/// renders it through `render_trace`, so the assertions track the CLI's
/// golden-path ones (`engine`, `replay`, a duration).
#[tokio::test]
async fn trace_page_lists_engine_span() {
    let (app, _state, tmp) = app();
    let request_id = uuid::Uuid::now_v7();
    let logs_dir = tmp.path().join("data").join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();
    let mut content = fixture_lines(&request_id).join("\n");
    content.push('\n');
    std::fs::write(logs_dir.join("cauce-2026-10-28.jsonl"), content).unwrap();

    let (status, body) = get_html(&app, &format!("/trace/{request_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("engine=replay"),
        "engine span missing from trace page:\n{body}"
    );
    assert!(
        body.contains("ms"),
        "span durations missing from trace page:\n{body}"
    );
    assert!(
        body.contains(&request_id.to_string()),
        "traced id missing: {body}"
    );
}

/// A non-UUID trace id is a 400 (the CLI's usage-error contract), not a
/// panic or a 404 — the routes-table probe hits this path too.
#[tokio::test]
async fn trace_page_rejects_bad_id() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/trace/replay").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("not a UUID"), "{body}");
}

/// A request id with no log records renders the "no records" timeline, not
/// an error page — retention may have already dropped the files.
#[tokio::test]
async fn trace_page_empty_when_no_records() {
    let (app, _state, tmp) = app();
    std::fs::create_dir_all(tmp.path().join("data").join("logs")).unwrap();
    let request_id = uuid::Uuid::now_v7();
    let (status, body) = get_html(&app, &format!("/trace/{request_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("no records found"), "{body}");
}
