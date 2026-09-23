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
use cauce_core::config::{Config, EnvMap};
use cauce_core::{AuditRow, SearchPipeline, StoreTuning};
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
        body.contains("no audit rows match actor &#34;cli&#34;"),
        "filtered empty state should name the active filter: {body}"
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

/// The HTML and JSON surfaces share default limits and empty-filter handling.
#[tokio::test]
async fn audit_page_shares_api_defaults_and_empty_filters() {
    let (app, state, _tmp) = app();
    for index in 0..55 {
        state
            .store()
            .audit(AuditRow {
                id: None,
                ts: chrono::Utc::now(),
                actor: "ui".to_string(),
                action: "audit.test".to_string(),
                target: index.to_string(),
                details: Value::Null,
                request_id: None,
            })
            .await
            .expect("seed audit row");
    }

    let (status, api_body) = get_json(&app, "/api/audit?actor=&action=").await;
    assert_eq!(status, StatusCode::OK);
    let api_rows = api_body.as_array().expect("audit rows");
    assert_eq!(api_rows.len(), 50, "API default limit changed: {api_body}");

    let (status, body) = get_html(&app, "/audit?actor=&action=").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("50 rows"),
        "HTML must use the API default limit and treat empty filters equally: {body}"
    );
    // The listing is exactly at the limit: the cap note must show.
    assert!(
        body.contains("showing the newest 50"),
        "cap note missing: {body}"
    );
    // All 50 seeded rows have null details: no details toggle may render.
    assert!(
        !body.contains("<details"),
        "null details must omit the toggle: {body}"
    );
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

/// The filter `<select>`s are populated from the distinct actors/actions
/// the store has seen (`Store::audit_facets`), each with an `any` default.
#[tokio::test]
async fn audit_page_selects_populate_from_facets() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-facets-seed").await;

    let (status, body) = get_html(&app, "/audit").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<select name=\"actor\""), "{body}");
    assert!(body.contains("<select name=\"action\""), "{body}");
    assert!(
        body.contains("<option value=\"ui\""),
        "actor option missing: {body}"
    );
    assert!(
        body.contains("<option value=\"cache.delete\""),
        "action option missing: {body}"
    );
}

/// Filtered listings say `N rows matching` and the filtered-empty copy
/// names every active filter.
#[tokio::test]
async fn audit_page_filtered_count_and_empty_copy() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-count-seed").await;

    let (status, body) = get_html(&app, "/audit?actor=ui").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("1 rows matching"), "{body}");

    let (status, body) = get_html(&app, "/audit?actor=ui&action=config.put").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("0 rows matching"), "{body}");
    assert!(
        body.contains("no audit rows match actor &#34;ui&#34; and action &#34;config.put&#34;"),
        "filtered empty must name both filters: {body}"
    );
}

/// W2-06 acceptance, second half: `/trace/<id>` of a real replay search
/// through the app lists the `replay` engine span in both the timeline and
/// the HTML spans list. The search runs under a JSONL subscriber pointed at
/// the test's logs dir, the same wiring `cauce serve` installs.
#[tokio::test]
async fn trace_page_lists_replay_span() {
    let (app, _state, tmp) = app();
    let logs_dir = tmp.path().join("data").join("logs");
    std::fs::create_dir_all(&logs_dir).unwrap();

    let obs = cauce_server::observability::ObservabilityConfig {
        logs_dir: logs_dir.clone(),
        ..Default::default()
    };
    let (dispatch, guard) = cauce_server::observability::build(&obs).expect("obs build");
    let request_id = {
        let _default = tracing::dispatcher::set_default(&dispatch);
        let request = Request::builder()
            .method(Method::GET)
            .uri("/api/search?q=trace-replay")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(request).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        resp.headers()["x-request-id"].to_str().unwrap().to_string()
    };
    drop(guard); // flush the non-blocking JSONL writer

    let (status, body) = get_html(&app, &format!("/trace/{request_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(&request_id), "traced id missing: {body}");
    // The summary names the pipeline work, not the middleware root:
    // `search "q" · ts · ms · outcome`, not `request · ts · ms · ok`.
    assert!(
        body.contains("search \"trace-replay\""),
        "summary should name the pipeline kind and query: {body}"
    );
    // The engine span itself appears in the timeline and heads one spans
    // list entry — not just the string "replay" somewhere on the page.
    assert!(
        body.contains("engine=replay"),
        "replay engine span missing from the timeline:\n{body}"
    );
    assert!(
        body.contains("<details class=\"span\"><summary>replay"),
        "spans list should open with the replay engine span: {body}"
    );
    // Footer carries the page render's own request id, distinct from the
    // traced id.
    assert!(body.contains("class=\"request-id\""), "{body}");
}

/// A malformed trace id renders the page frame with `that is not a request
/// id` at 400; the JSON arm keeps the ApiError envelope.
#[tokio::test]
async fn trace_page_rejects_bad_id() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/trace/not-an-id").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("that is not a request id"), "{body}");
    assert!(body.contains("Trace"), "{body}");

    let (status, body) = get_json(&app, "/trace/not-an-id").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "bad_request", "{body}");
}

/// A well-formed id with no log records is a 404 inside the page frame,
/// with the `logs.retention_days` hint.
#[tokio::test]
async fn trace_page_not_found_frame() {
    let (app, _state, tmp) = app();
    std::fs::create_dir_all(tmp.path().join("data").join("logs")).unwrap();
    let request_id = uuid::Uuid::now_v7();
    let (status, body) = get_html(&app, &format!("/trace/{request_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body.contains("no trace for this request id. traces are kept for logs.retention_days days"),
        "retention hint missing: {body}"
    );
}
