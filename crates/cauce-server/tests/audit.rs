//! `/api/audit` filter/listing tests and the `/trace/{id}` page (W2-06;
//! the `/audit` page moved to `/app/admin?tab=audit` in FX-05).
//!
//! Acceptance: a cache delete made the way W2-04's page makes it
//! (`DELETE /api/cache/{key}` with `X-Cauce-Client: ui`) shows on `/api/audit`
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
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::{Config, EnvMap};
use cauce_core::{AuditRow, SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

mod support;
use support::*;

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
/// lands on `/api/audit` with actor `ui`, newest first, carrying the
/// request id the audit tab links to `/trace/<id>` (the `/audit` page
/// itself moved to `/app/admin?tab=audit` in FX-05).
#[tokio::test]
async fn audit_api_lists_ui_cache_delete() {
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

    let (status, body) = get_json(&app, "/api/audit").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("audit rows");
    let actions: Vec<&str> = rows.iter().filter_map(|r| r["action"].as_str()).collect();
    assert!(
        actions.contains(&"engine.reset"),
        "reset row missing: {body}"
    );
    assert!(
        actions.contains(&"cache.delete"),
        "delete row missing: {body}"
    );
    assert!(
        actions.iter().position(|a| *a == "engine.reset")
            < actions.iter().position(|a| *a == "cache.delete"),
        "rows should be newest first:\n{body}"
    );
    let delete = rows
        .iter()
        .find(|r| r["action"].as_str() == Some("cache.delete"))
        .expect("delete row");
    assert_eq!(delete["actor"].as_str(), Some("ui"));
    assert!(
        delete["request_id"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "request id the tab links to /trace/<id>: {body}"
    );
    let reset = rows
        .iter()
        .find(|r| r["action"].as_str() == Some("engine.reset"))
        .expect("reset row");
    assert!(
        reset["details"].is_object() && !reset["details"].as_object().unwrap().is_empty(),
        "reset details carry the engine id: {body}"
    );
}

/// `actor` and `action` URL params filter the JSON listing; a filter
/// that matches nothing answers an empty array, not an error.
#[tokio::test]
async fn audit_api_filters_by_actor_and_action() {
    let (app, _state, _tmp) = app();
    ui_cache_delete(&app, "audit-filter-seed").await;

    let (status, body) = get_json(&app, "/api/audit?actor=ui").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["action"].as_str() == Some("cache.delete")),
        "{body}"
    );

    let (status, body) = get_json(&app, "/api/audit?actor=cli").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0, "{body}");

    let (status, body) = get_json(&app, "/api/audit?action=cache.delete").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{body}");
    // `cache.delete` writes `{}` details — the wire keeps the empty
    // object; the SPA's toggle check treats it like `null`.
    assert_eq!(rows[0]["details"], serde_json::json!({}), "{body}");

    let (status, body) = get_json(&app, "/api/audit?action=config.put").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0, "{body}");

    // Unknown params are 400s.
    let (status, _) = get_json(&app, "/api/audit?bogus=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// The listing's default limit is 50 rows.
#[tokio::test]
async fn audit_api_default_limit_and_empty_filters() {
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
    assert!(
        api_rows.iter().all(|r| r["details"].is_null()),
        "null details stay null on the wire: {api_body}"
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
    // Callsite `Interest` is a process-global cache (tracing-core): a
    // callsite first registered on a thread with no dispatcher caches
    // `Interest::never`, silently disabling that span for every subscriber.
    // Parallel tests run searches that can touch the `engine` callsite
    // during this test's window, so a thread-local `set_default` alone is
    // racy. A global default makes every thread's lazy registration
    // evaluate against this dispatch instead; `set_default` still scopes
    // the request itself. Only this test builds a dispatch, so the
    // once-per-process global cannot conflict; the `.expect` pins that
    // single-dispatcher invariant. `rebuild_interest_cache` then drops
    // any `Interest::never` a callsite cached before the global landed,
    // closing the residual registration window.
    tracing::dispatcher::set_global_default(dispatch.clone())
        .expect("only this test installs a global dispatcher");
    tracing::callsite::rebuild_interest_cache();
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
