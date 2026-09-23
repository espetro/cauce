//! `GET /api/engines` + `POST /api/engines/{id}/reset` (W1-06) and the
//! W2-05 additions: the `/engines` page over the same handler, the
//! enable/disable pair, and the `Accept: text/html` test fragment.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{EngineId, SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Serialises tests that mutate process env (`CAUCE_CONFIG_DIR`), same
/// discipline as `tests/routes.rs`.
static ENV_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

fn state_with(replay: ReplayOpts) -> (Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(replay))],
    ));
    let state = AppState::new(pipeline, store, Config::default());
    (build_router(state), tmp)
}

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
    let (router, _tmp) = state_with(ReplayOpts::default());

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
    let (router, _tmp) = state_with(ReplayOpts {
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
// W2-05 `/engines` page, enable/disable pair, inline test fragment
// ---------------------------------------------------------------------

/// The page renders one card per configured engine — the built-ins
/// `replay` and `ddgs` here — with kind/tier, the breaker chip, the stats
/// list, the summary line and the full request id footer.
#[tokio::test]
async fn engines_page_lists_cards() {
    let (router, _tmp) = state_with(ReplayOpts::default());

    let (status, body, ct) = fetch(&router, "GET", "/engines", &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("text/html"), "{ct}");

    // Summary line: 2 configured built-ins, 1 enabled (ddgs).
    assert!(body.contains("2 configured"), "{body}");
    assert!(body.contains("1 enabled"), "{body}");
    assert!(!body.contains("breaker open"), "{body}");

    // One card per configured engine, spec fields present.
    for needle in [
        "id=\"engine-dreplay\"",
        "id=\"engine-dddgs\"",
        "data-request-id",
        ">Closed<",
        "enabled",
        "ewma",
        "last ok",
        "last error",
        "p95",
        "reliability",
        "requests today",
        "reset breaker",
        "/api/engines/replay/reset",
        // ddgs is configured but not in the running pipeline.
        "not running",
        // replay's toggle offers the action that would happen.
        "/api/engines/replay/enable",
        "/api/engines/ddgs/disable",
        // The test form hits the shared data path.
        "hx-get=\"/api/search\"",
    ] {
        assert!(body.contains(needle), "missing {needle:?} in {body}");
    }

    // Full request id in the footer (copyable), not the 8-char short id.
    let footer_id = body
        .split("class=\"request-id\"")
        .nth(1)
        .and_then(|s| s.split('>').nth(1))
        .and_then(|s| s.split('<').next())
        .unwrap_or("");
    assert!(
        footer_id.len() > 8,
        "footer should carry the full id: {body}"
    );
}

/// `/engines` and `GET /api/engines` share one handler: a JSON `Accept`
/// on the page path gets the same rows the API returns.
#[tokio::test]
async fn engines_page_path_negotiates_json() {
    let (router, _tmp) = state_with(ReplayOpts::default());

    let (status, body) = call(&router, "GET", "/engines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.as_array()
            .expect("rows")
            .iter()
            .any(|r| r["engine"] == "replay")
    );

    let (status, body, _) = fetch(
        &router,
        "GET",
        "/engines",
        &[("accept", "application/json")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Value = serde_json::from_str(&body).expect("JSON body");
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["engine"] == "replay")
    );
}

/// W2-05 acceptance at page level: a `blocked` replay's failed call
/// opens the breaker, `/engines` shows `Open` with the countdown and the
/// summary counts it, and the page's reset action swaps in the
/// `HalfOpen` card.
#[tokio::test]
async fn engines_page_open_breaker_resets_to_half_open() {
    let (router, _tmp) = state_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });

    let (status, _) = call(&router, "GET", "/api/search?q=br1").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let (status, body, _) = fetch(&router, "GET", "/engines", &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(">Open<"), "chip should read Open: {body}");
    assert!(
        body.contains("retries in"),
        "open chip needs the countdown: {body}"
    );
    assert!(
        body.contains("1 breaker open"),
        "summary should count it: {body}"
    );

    // The card's reset action (HTMX, actor `ui`) returns the swapped card.
    let (status, card, _) = fetch(
        &router,
        "POST",
        "/api/engines/replay/reset",
        &[("hx-request", "true"), ("x-cauce-client", "ui")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{card}");
    assert!(card.contains("id=\"engine-dreplay\""), "{card}");
    assert!(card.contains(">HalfOpen<"), "{card}");
    assert!(card.contains("probing"), "{card}");
    assert!(card.contains("data-request-id"), "{card}");

    // The reset is audited with the ui actor (checkpoint #28).
    let (_, audit) = call(&router, "GET", "/api/audit").await;
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "engine.reset" && r["actor"] == "ui"),
        "audit rows: {audit}"
    );

    // A page reload agrees with the swapped fragment.
    let (status, body, _) = fetch(&router, "GET", "/engines", &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(">HalfOpen<"), "{body}");
    assert!(!body.contains("breaker open"), "{body}");
}

/// The card's test form submits `GET /api/search?engines=<id>` under
/// `Accept: text/html`; the handler's fragment arm answers the meta line
/// plus the shared `results.html` list (checkpoint #29).
#[tokio::test]
async fn api_search_html_returns_test_fragment() {
    let (router, _tmp) = state_with(ReplayOpts::default());

    let (status, body, ct) = fetch(
        &router,
        "GET",
        "/api/search?q=test&engines=replay",
        &[("accept", "text/html")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("text/html"), "{ct}");
    assert!(body.contains("results ·"), "meta line: {body}");
    assert!(body.contains(" ms"), "elapsed in meta line: {body}");
    // Same result anatomy as the search page's `results.html`.
    assert!(body.contains("<article"), "{body}");
    assert!(body.contains("hx-post=\"/api/click\""), "{body}");

    // The JSON arm is unchanged by the negotiation.
    let (status, body) = call(&router, "GET", "/api/search?q=test&engines=replay").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body["results"].as_array().unwrap().is_empty());
}

/// A failed test query renders the engine's error class: `blocked` for a
/// blocked engine, `breaker open` once the failure has opened the
/// breaker. HTMX callers get the swap-friendly 200; a direct HTML caller
/// gets the real status.
#[tokio::test]
async fn api_search_html_fragment_names_error_class() {
    let (router, _tmp) = state_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });

    let (status, body, _) = fetch(
        &router,
        "GET",
        "/api/search?q=x&engines=replay",
        &[("accept", "text/html")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.contains("blocked"), "class line: {body}");

    // The failure opened the breaker: the pinned call is skipped now.
    let (status, body, _) = fetch(
        &router,
        "GET",
        "/api/search?q=x&engines=replay",
        &[("accept", "text/html"), ("hx-request", "true")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("breaker open"), "class line: {body}");
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
    let (router, _tmp) = state_with(ReplayOpts::default());

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

/// An HTMX toggle answers the re-rendered card — enabled flag flipped,
/// the `saved; applies after restart` hint shown — so the page swaps it
/// without a reload.
#[tokio::test]
async fn engine_toggle_hx_returns_card_with_saved_hint() {
    let _guard = env_lock().await;
    let cfg_dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", cfg_dir.path());
    }
    let (router, _tmp) = state_with(ReplayOpts::default());

    let (status, card, _) = fetch(
        &router,
        "POST",
        "/api/engines/ddgs/disable",
        &[("hx-request", "true"), ("x-cauce-client", "ui")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{card}");
    assert!(card.contains("id=\"engine-dddgs\""), "{card}");
    assert!(card.contains("saved; applies after restart"), "{card}");
    // The flipped card offers the reverse action.
    assert!(card.contains("/api/engines/ddgs/enable"), "{card}");

    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
    }
}

/// Dotted and dashed engine ids are both legal, so the card element ids
/// and the `hx-target` selectors must be dot-free *and* injective: a raw
/// `hx-target="#engine-a.b"` parses `.b` as a class selector and the
/// reset/toggle/test actions are silently dead, and a naive `.` -> `-`
/// swap would collapse `a.b` and `a-b` onto one element id. The cards
/// take the shared `encode_id` (`a.b` -> `engine-da-db`, `a-b` ->
/// `engine-da--b`); display text and the `engines` form value keep the
/// raw id.
#[tokio::test]
async fn engine_card_selectors_are_dot_free_and_unique() {
    let raw: toml::Value = toml::from_str(
        "[[engines]]\n\
         id = \"a.b\"\n\
         kind = \"replay\"\n\n\
         [[engines]]\n\
         id = \"a-b\"\n\
         kind = \"replay\"\n",
    )
    .expect("toml parses");
    let cfg = Config::from_raw(&raw, &BTreeMap::new()).expect("config parses");
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    // Registered ids are `tracked`, so the reset button (one of the three
    // `hx-target` slots) renders on the dotted/dashed cards too.
    pipeline.health().register(&EngineId::from("a.b"));
    pipeline.health().register(&EngineId::from("a-b"));
    let router = build_router(AppState::new(pipeline, store, cfg));

    let (status, page, _) = fetch(&router, "GET", "/engines", &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    for sel in [
        "engine-da-db",
        "engine-da--b",
        "test-da-db",
        "test-da--b",
        "engine-dreplay",
    ] {
        assert!(
            page.contains(&format!("id=\"{sel}\"")),
            "missing id {sel}: {page}"
        );
        assert!(
            page.contains(&format!("hx-target=\"#{sel}\"")),
            "missing hx-target {sel}: {page}"
        );
    }
    // The `engines` form value stays the raw id — it is an API param,
    // not a selector.
    assert!(page.contains("name=\"engines\" value=\"a.b\""), "{page}");
    assert!(page.contains("name=\"engines\" value=\"a-b\""), "{page}");
    // And the action URLs keep the raw (urlencoded) path id.
    assert!(
        page.contains("hx-post=\"/api/engines/a.b/reset\""),
        "{page}"
    );
    // No two cards share an element id.
    let mut ids: Vec<&str> = page
        .split("id=\"engine-")
        .skip(1)
        .map(|s| s.split('"').next().unwrap_or(""))
        .collect();
    ids.sort_unstable();
    let total = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), total, "duplicate engine card ids: {ids:?}");
}

