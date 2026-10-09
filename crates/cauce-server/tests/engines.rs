//! `GET /api/engines` + `POST /api/engines/{id}/reset` (W1-06) and the
//! W2-05 additions: the engine enable/disable pair and the test-query
//! JSON path the `/app/admin?tab=engines` tab drives (the page moved
//! to the SPA in FX-05).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{BreakerState, EngineHealthRow, EngineId, SearchPipeline, Store, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

mod support;
use support::*;

async fn call(router: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let (status, body, _) = fetch(router, method, uri, &[]).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

/// Raw fetch with request headers; returns status, body text and the
/// `Content-Type` (page/fragment assertions look at the markup, not JSON).
async fn fetch(
    router: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, String, String) {
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .expect("response");
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        String::from_utf8_lossy(&bytes).into_owned(),
        content_type,
    )
}

/// The `replay` row of the shared `/api/engines` view (the built-in
/// `ddgs` entry sorts first in the id-ordered list).
fn replay_row(body: &Value) -> &Value {
    body.as_array()
        .expect("engine list")
        .iter()
        .find(|r| r["engine"] == "replay")
        .expect("a replay row")
}

/// Healthy engine: listed as `closed`; reset is a 200 that leaves the
/// engine in `half_open` (the next call is the single probe, per the W2-05
/// acceptance) and is audited; unknown ids 404.
#[tokio::test]
async fn engines_list_and_reset() {
    let (router, _state, _tmp) = app_with(ReplayOpts::default());

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("engine list");
    // `replay` (live) + `ddgs` (configured built-in, not running).
    assert_eq!(rows.len(), 2, "{body}");
    let row = replay_row(&body);
    assert_eq!(row["engine"], "replay");
    assert_eq!(row["breaker"], "closed");
    assert_eq!(row["failures"], 0);
    // The W2-05 card fields ride the same JSON body (screen spec: the
    // page and the API share one data plane).
    assert_eq!(row["kind"], "replay");
    assert_eq!(row["configured"], true);
    assert_eq!(row["live"], true);
    assert!(row["requests_today"].is_number());
    let ddgs = rows
        .iter()
        .find(|r| r["engine"] == "ddgs")
        .expect("ddgs row");
    assert_eq!(ddgs["kind"], "exec");
    assert_eq!(ddgs["live"], false);

    let (status, body) = call(&router, "POST", "/api/engines/replay/reset").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["engine"], "replay");
    assert_eq!(body["breaker"], "half_open");

    let (status, body) = call(&router, "GET", "/api/audit").await;
    assert_eq!(status, StatusCode::OK);
    let audit = body.as_array().unwrap();
    assert!(
        audit
            .iter()
            .any(|r| r["action"] == "engine.reset" && r["target"] == "replay"),
        "engine.reset audit row missing: {body}"
    );

    let (status, body) = call(&router, "POST", "/api/engines/nope/reset").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}

/// A `blocked` replay opens the breaker; `GET /api/engines` reports it,
/// the next search is breaker-skipped (503), and reset re-admits the
/// engine (which fails again and re-opens).
#[tokio::test]
async fn blocked_engine_reports_open_and_reset_readmits() {
    let (router, _state, _tmp) = app_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });

    let (status, body) = call(&router, "GET", "/api/search?q=br1").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK);
    let row = replay_row(&body);
    assert_eq!(row["breaker"], "open", "{row}");
    assert_eq!(row["failures"], 1);
    assert!(row["breaker_until"].is_string());
    assert!(row["last_error"].is_string());

    // Open engine is skipped, not called: 503 `breaker_open`.
    let (status, body) = call(&router, "GET", "/api/search?q=br2").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "breaker_open");

    // Reset half-opens the breaker; the next search is admitted as the
    // single probe (it fails once more and re-opens).
    let (status, body) = call(&router, "POST", "/api/engines/replay/reset").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["breaker"], "half_open");
    assert_eq!(body["failures"], 0);
    assert_eq!(body["breaker_until"], Value::Null);

    let (status, body) = call(&router, "GET", "/api/search?q=br3").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay_row(&body)["breaker"], json!("open"), "{body}");
}

// ---------------------------------------------------------------------
// W2-05 enable/disable pair, inline test fragment
// ---------------------------------------------------------------------

/// The engines admin tab runs its test query as
/// `GET /api/search?engines=<id>` — JSON under any Accept header now
/// (FX-06 dropped the fragment arm; an `Accept: text/html` caller gets
/// the same envelope).
#[tokio::test]
async fn api_search_answers_json_under_any_accept() {
    let (router, _state, _tmp) = app_with(ReplayOpts::default());

    let (status, body, ct) = fetch(
        &router,
        "GET",
        "/api/search?q=test&engines=replay",
        &[("accept", "text/html")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("application/json"), "{ct}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert!(!body["results"].as_array().unwrap().is_empty());
}

/// A failed test query maps the engine's error class onto the JSON
/// envelope: `upstream_failed` 502 for a blocked engine, `breaker_open`
/// 503 once the failure has opened the breaker (the swap-friendly 200
/// the HTMX card used to get is gone with the fragment arm — FX-06).
#[tokio::test]
async fn api_search_envelope_names_error_class() {
    let (router, _state, _tmp) = app_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });

    let (status, body) = call(&router, "GET", "/api/search?q=x&engines=replay").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["error"]["code"], "upstream_failed", "{body}");

    // The failure opened the breaker: the pinned call is skipped now.
    let (status, body) = call(&router, "GET", "/api/search?q=x&engines=replay").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "breaker_open", "{body}");
}

/// `POST /api/engines/{id}/enable|disable` writes `enabled` into
/// `config.toml` through the `PUT /api/config` discipline, audits
/// `engine.enable`/`engine.disable`, and 404s unknown ids.
#[tokio::test]
async fn engine_enable_disable_writes_config_and_audits() {
    let _guard = env_lock().await;
    let cfg_dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", cfg_dir.path());
    }
    let (router, _state, _tmp) = app_with(ReplayOpts::default());

    let (status, body) = call(&router, "POST", "/api/engines/ddgs/disable").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "ddgs");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["effective_after_restart"], true);

    // The file carries the flipped flag on a synthesized built-in entry.
    let written = std::fs::read_to_string(cfg_dir.path().join("config.toml")).expect("config.toml");
    assert!(written.contains("ddgs"), "{written}");
    let cfg = Config::load().expect("reload config");
    assert!(!cfg.engine("ddgs").expect("ddgs entry").enabled);
    assert!(!cfg.engine("replay").expect("replay entry").enabled);

    let (status, body) = call(&router, "POST", "/api/engines/ddgs/enable").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);
    let cfg = Config::load().expect("reload config");
    assert!(cfg.engine("ddgs").expect("ddgs entry").enabled);

    let (_, audit) = call(&router, "GET", "/api/audit").await;
    let rows = audit.as_array().unwrap();
    for action in ["engine.disable", "engine.enable"] {
        assert!(
            rows.iter()
                .any(|r| r["action"] == action && r["target"] == "ddgs"),
            "{action} audit row missing: {audit}"
        );
    }

    let (status, body) = call(&router, "POST", "/api/engines/nope/disable").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");

    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
    }
}

/// A file entry whose `id` leaf is a `${env:...}` template still gets the
/// in-place patch (the match is positional against the resolved
/// `cfg.engines`), so the written `config.toml` keeps every template
/// verbatim — critically, resolved secret leaves can never be persisted.
/// The previous literal-`id` match missed templated ids and fell through
/// to serializing the resolved entry, leaking secrets into the file.
#[tokio::test]
async fn engine_toggle_preserves_templates_and_never_writes_secrets() {
    let _guard = env_lock().await;
    let cfg_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        cfg_dir.path().join("config.toml"),
        "[[engines]]\n\
         id = \"${env:CAUCE_TEST_ENGINE_ID}\"\n\
         kind = \"replay\"\n\
         enabled = false\n\
         [engines.env]\n\
         API_KEY = \"${env:CAUCE_TEST_SECRET}\"\n",
    )
    .expect("write config.toml");
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", cfg_dir.path());
        std::env::set_var("CAUCE_TEST_ENGINE_ID", "replay");
        std::env::set_var("CAUCE_TEST_SECRET", "s3cret-value");
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let router = build_router(AppState::new(
        pipeline,
        store,
        Config::load().expect("config loads"),
    ));

    let (status, body) = call(&router, "POST", "/api/engines/replay/enable").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);

    let written = std::fs::read_to_string(cfg_dir.path().join("config.toml")).expect("config.toml");
    // The flag flipped on the templated entry, in place.
    assert!(written.contains("enabled = true"), "{written}");
    // Templates round-trip verbatim; nothing resolved was persisted.
    assert!(written.contains("${env:CAUCE_TEST_ENGINE_ID}"), "{written}");
    assert!(written.contains("${env:CAUCE_TEST_SECRET}"), "{written}");
    assert!(!written.contains("s3cret-value"), "{written}");

    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
        std::env::remove_var("CAUCE_TEST_ENGINE_ID");
        std::env::remove_var("CAUCE_TEST_SECRET");
    }
}

/// The same pre-validation row must not reach `StatsSnapshot.engines`
/// either: the dashboard's engines table would render a `card_anchor`
/// link to a card `/engines` never produces, and `/api/stats` would
/// diverge from the filtered `/api/engines`.
#[tokio::test]
async fn persisted_health_row_with_invalid_id_is_absent_from_stats() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    store
        .put_health(&EngineHealthRow {
            engine: EngineId::from("bad id"),
            ewma_ms: 12.0,
            failures: 1,
            breaker: BreakerState::Closed,
            breaker_until: None,
            last_ok_at: None,
            last_error: None,
        })
        .await
        .expect("put_health");
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    pipeline.load_health().await.expect("health rows load");
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    let (status, body) = call(&router, "GET", "/api/stats").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let engines = body["engines"].as_array().expect("engines rows");
    assert!(
        !engines.iter().any(|r| r["engine"] == "bad id"),
        "invalid persisted id leaked into /api/stats engines: {body}"
    );
}

/// `POST /api/engines/{id}/reset` on a persisted-but-invalid id 404s like
/// any unknown id: the handler must not reset the tracker, re-persist the
/// phantom row through `put_health`, or audit it.
#[tokio::test]
async fn engine_reset_rejects_invalid_persisted_id() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    store
        .put_health(&EngineHealthRow {
            engine: EngineId::from("bad id"),
            ewma_ms: 12.0,
            failures: 1,
            breaker: BreakerState::Closed,
            breaker_until: None,
            last_ok_at: None,
            last_error: None,
        })
        .await
        .expect("put_health");
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    pipeline.load_health().await.expect("health rows load");
    let router = build_router(AppState::new(pipeline, store.clone(), Config::default()));

    let (status, body) = call(&router, "POST", "/api/engines/bad%20id/reset").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");

    // No `put_health` rewrite: a reset would have zeroed the failure
    // count and persisted `half_open`.
    let rows = store.health().await.expect("health rows");
    let row = rows
        .iter()
        .find(|r| r.engine.as_str() == "bad id")
        .expect("persisted row");
    assert_eq!(row.failures, 1, "{row:?}");
    assert_eq!(row.breaker, BreakerState::Closed, "{row:?}");

    let (_, audit) = call(&router, "GET", "/api/audit").await;
    assert!(
        !audit
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "engine.reset" && r["target"] == "bad id"),
        "phantom reset audited: {audit}"
    );
}
