//! `GET /api/config` reads and the urlencoded `PUT /api/config` form path//! (W2-07; the `/settings` page itself moved to `/app/settings` in FX-05).
//!
//! Acceptance: a page test edits `search.deadline_ms`, saves, reloads and
//! sees the value; the `${env:PROVIDER_API_KEY}` template survives a save
//! round-trip byte-for-byte.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;

mod support;
use support::*;

/// A replay-engine app whose `Config` was `load()`ed against the sandbox env.
fn app(tmp: &tempfile::TempDir) -> Router {
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, Config::load().expect("config"));
    build_router(state)
}

fn saved_config(tmp: &tempfile::TempDir) -> String {
    std::fs::read_to_string(tmp.path().join("cfg/config.toml")).expect("config file")
}

/// Acceptance: edit `search.deadline_ms` on the page, save, reload, see it.
#[tokio::test]
async fn deadline_edit_saves_and_reloads() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[search]\ndeadline_ms = 3000\n");
    let app = app(&tmp);

    let (status, body) = put_form(&app, "search.deadline_ms=1234", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(json["search"]["deadline_ms"], 1234);
    // `search.*` is hot-applicable: the save applies in place, so nothing
    // is left pending for a restart.
    assert_eq!(json["effective_after_restart"], false);
    assert_eq!(json["applied"], serde_json::json!(["search.deadline_ms"]));
    assert_eq!(json["requires_restart"], serde_json::json!([]));

    let on_disk = saved_config(&tmp);
    assert!(
        on_disk.contains("deadline_ms = 1234"),
        "file should carry the new deadline: {on_disk}"
    );

    let (status, json) = get_json(&app, "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["search"]["deadline_ms"], 1234,
        "reloaded config should carry the saved deadline: {json}"
    );
    clear_env();
}

/// #233: the `[ai]` loop knobs save through the form whitelist, land in
/// `config.toml` hot-applied and render back on reload.
#[tokio::test]
async fn ai_loop_knobs_save_and_reload() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[ai]\nmax_turns = 8\n");
    let app = app(&tmp);

    let (status, body) = put_form(
        &app,
        "ai.max_turns=4&ai.max_searches=2&ai.provider_budget_s=30",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(json["ai"]["max_turns"], 4);
    assert_eq!(json["ai"]["max_searches"], 2);
    assert_eq!(json["ai"]["provider_budget_s"], 30);
    // `ai.*` is hot-applicable (#236): the knobs apply in place.
    assert_eq!(json["requires_restart"], serde_json::json!([]));

    let on_disk = saved_config(&tmp);
    for needle in [
        "max_turns = 4",
        "max_searches = 2",
        "provider_budget_s = 30",
    ] {
        assert!(
            on_disk.contains(needle),
            "file should carry {needle}: {on_disk}"
        );
    }

    let (status, json) = get_json(&app, "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ai"]["max_turns"], 4, "{json}");
    assert_eq!(json["ai"]["max_searches"], 2, "{json}");
    assert_eq!(json["ai"]["provider_budget_s"], 30, "{json}");
    clear_env();
}

#[tokio::test]
async fn engine_fields_write_file_entries() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = put_form(
        &app,
        "engines.replay.enabled=true&engines.replay.tier=2&engines.replay.egress.proxy=http%3A%2F%2F127.0.0.1%3A8888",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let on_disk = saved_config(&tmp);
    let tree = toml::from_str::<toml::Value>(&on_disk).expect("saved TOML parses");
    let replay = tree["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some("replay"))
        .expect("replay entry written");
    assert_eq!(replay["enabled"].as_bool(), Some(true));
    assert_eq!(replay["tier"].as_integer(), Some(2));
    assert_eq!(
        replay["egress"]["proxy"].as_str(),
        Some("http://127.0.0.1:8888")
    );
    clear_env();
}

/// An id outside `[A-Za-z0-9._-]+` is rejected at config parse, so a TOML
/// `PUT /api/config` carrying one fails in-memory validation (400, error
/// naming the id and the allowed charset) and never reaches the file.
#[tokio::test]
async fn invalid_engine_id_charset_is_rejected() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::PUT)
            .uri("/api/config")
            .header("Content-Type", "application/toml")
            .body(Body::from(
                "[[engines]]\nid = \"a b\"\nkind = \"replay\"\n".to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json envelope");
    assert_eq!(json["error"]["code"], "invalid_config");
    let msg = json["error"]["message"].as_str().unwrap();
    assert!(msg.contains("a b"), "{msg}");
    assert!(msg.contains("[A-Za-z0-9._-]+"), "{msg}");
    assert_eq!(
        saved_config(&tmp),
        "",
        "a rejected config must never be written"
    );
    clear_env();
}

/// `config.put` audit rows name the changed keys and the `ui` actor.
#[tokio::test]
async fn save_writes_audit_with_changed_keys() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[search]\ndeadline_ms = 3000\n");
    let app = app(&tmp);

    let (status, body) = put_form(&app, "search.deadline_ms=1234", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/api/audit?action=config.put")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Value = serde_json::from_str(&body).expect("audit rows");
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"].as_str() == Some("config.put"))
        .expect("config.put audit row");
    assert_eq!(row["actor"].as_str(), Some("ui"), "{row}");
    assert_eq!(
        row["details"]["changed"],
        serde_json::json!(["search.deadline_ms"]),
        "{row}"
    );
    clear_env();
}

/// A file-literal secret renders as typed on the page (the owner's own
/// config file); the `restore_redacted` pass still keeps a `<redacted>`
/// submission from clobbering the secret, so the audit row must not report
/// `ai.api_key` as changed.
#[tokio::test]
async fn literal_secret_renders_and_redacted_restore_holds() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[ai]\napi_key = \"sk-file-literal\"\n");
    let app = app(&tmp);

    let (status, json) = get_json(&app, "/api/config").await;
    assert_eq!(status, StatusCode::OK, "{json}");
    // The wire redacts secret leaves (`<redacted>`); a literal in the
    // file never leaks over JSON — it stays editable-by-placeholder.
    assert_eq!(
        json["ai"]["api_key"].as_str(),
        Some("<redacted>"),
        "a literal secret must be redacted on the wire: {json}"
    );

    // A stale page (or a JSON API client) can still submit the decoded
    // `<redacted>` placeholder verbatim; the restore pass writes the real
    // secret back rather than persisting the literal.
    let (status, body) = put_form(&app, "ai.api_key=%3Credacted%3E", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        saved_config(&tmp).contains("api_key = \"sk-file-literal\""),
        "the restore must write the file secret back"
    );

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/api/audit?action=config.put")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Value = serde_json::from_str(&body).expect("audit rows");
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"].as_str() == Some("config.put"))
        .expect("config.put audit row");
    assert_eq!(
        row["details"]["changed"],
        serde_json::json!([]),
        "a restored redacted leaf is no change: {row}"
    );
    clear_env();
}

/// Under `CAUCE_ENGINES=replay` the `enabled` checkboxes render as plain
/// text (`enabled` / `disabled`) — no `engines.<id>.enabled` input exists
/// at all, so a browser submits no enabled pair and a full-form save
/// creates the tier stanza without baking the pinned flag into the file.
#[tokio::test]
async fn pinned_engine_tier_edit_writes_no_enabled() {
    let _guard = env_lock().await;
    clear_env();
    // SAFETY: serialized by ENV_LOCK.
    unsafe { std::env::set_var("CAUCE_ENGINES", "replay") };
    let tmp = config_env("");
    let app = app(&tmp);

    // The form shape the SPA submits while `CAUCE_ENGINES` pins the flags:
    // every field except `engines.*.enabled`.
    let (status, body) = put_form(
        &app,
        concat!(
            "search.deadline_ms=5000&search.ttl_s=60",
            "&engines.replay.tier=2&engines.replay.egress.proxy=",
            "&engines.ddgs.tier=&engines.ddgs.egress.proxy=",
            "&admission.max_wait_ms=250&admission.max_concurrent_per_engine=4",
            "&logs.retention_days=14&ai.base_url=&ai.api_key=&ai.model=",
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let on_disk = saved_config(&tmp);
    let tree = toml::from_str::<toml::Value>(&on_disk).expect("saved TOML parses");
    let replay = tree["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some("replay"))
        .expect("replay stanza written");
    assert_eq!(replay["tier"].as_integer(), Some(2));
    assert!(
        !on_disk.contains("enabled"),
        "the CAUCE_ENGINES pin must not persist anywhere: {on_disk}"
    );
    assert!(
        replay.get("env").is_none(),
        "env must never serialise: {on_disk}"
    );
    clear_env();
}
