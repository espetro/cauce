//! Hot config apply (issue #225): a save through `PUT /api/config` or
//! the engine enable/disable endpoints re-points the running runtime —
//! engine fan-out, AI wiring, deadline/hedge/cache knobs — and the next
//! request honours the new values with no process restart.
//!
//! Acceptance: save → the next search observes the new value (deadline
//! hit, engine gone from the fan-out); `server.*`/`auth.*`/`logs.*`
//! still report `requires_restart`; the config file and the
//! `config.put` audit row both land.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::Router;
use axum::http::StatusCode;
use cauce_core::config::Config;
use serde_json::Value;

mod support;
use support::*;

/// `config_env` + `app_with_factory`: a router over a file-layer config
/// whose saves rebuild the runtime, like `cauce serve`.
fn app(toml_src: &str) -> (Router, tempfile::TempDir, tempfile::TempDir) {
    let tmp = config_env(toml_src);
    let (router, _state, db_tmp) = app_with_factory(Config::load().expect("config"));
    (router, tmp, db_tmp)
}

async fn search_meta(router: &Router, q: &str) -> Value {
    let (status, _headers, body) = get(router, &format!("/api/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["meta"].clone()
}

fn engine_ids(meta: &Value) -> Vec<String> {
    meta["engines_used"]
        .as_array()
        .expect("engines_used")
        .iter()
        .map(|r| r["engine"].as_str().unwrap().to_string())
        .collect()
}

/// save `search.deadline_ms` → the very next search request enforces it.
#[tokio::test]
async fn deadline_change_applies_on_next_search() {
    let _guard = env_lock().await;
    clear_env();
    // `fast` survives the tightened deadline so the response stays a 200
    // with `deadline_hit` on `slow`; `slow` would 502 the whole request
    // alone. ttl off so the second request always re-runs the fan-out.
    let (router, _cfg_tmp, _db_tmp) = app("[search]\ndeadline_ms = 2000\nttl_s = 0\n\n\
         [[engines]]\nid = \"fast\"\nkind = \"replay\"\n\n\
         [[engines]]\nid = \"slow\"\nkind = \"replay\"\n\n\
         [engines.params]\nlatency_ms = \"250\"\n");

    let meta = search_meta(&router, "alpha").await;
    assert_eq!(meta["deadline_hit"], false, "{meta}");

    // 100 ms: inside `slow`'s 250 ms replay latency, outside `fast`'s.
    let (status, body) = put_form(&router, "search.deadline_ms=100", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(json["applied"], serde_json::json!(["search.deadline_ms"]));
    assert_eq!(json["requires_restart"], serde_json::json!([]));
    assert_eq!(json["effective_after_restart"], false);

    // No restart: the next request hits a rebuilt pipeline whose deadline
    // the 250 ms replay engine cannot meet.
    let meta = search_meta(&router, "beta").await;
    assert_eq!(meta["deadline_hit"], true, "{meta}");
    let slow = meta["engines_used"]
        .as_array()
        .expect("engines_used")
        .iter()
        .find(|r| r["engine"] == "slow")
        .expect("slow report");
    assert_eq!(
        slow["status"],
        serde_json::json!({"failed": "timeout"}),
        "{slow}"
    );
    clear_env();
}

/// `engines.<id>.enabled` over the JSON API: the fan-out shrinks (and
/// grows back) without a restart, on both save paths — `PUT /api/config`
/// and `POST /api/engines/{id}/(en|dis)able`.
#[tokio::test]
async fn engine_toggle_rebuilds_fanout_without_restart() {
    let _guard = env_lock().await;
    clear_env();
    let (router, tmp, _db_tmp) = app("[[engines]]\nid = \"a\"\nkind = \"replay\"\n\n\
         [[engines]]\nid = \"b\"\nkind = \"replay\"\n");

    let meta = search_meta(&router, "one").await;
    assert_eq!(engine_ids(&meta), ["a", "b"], "{meta}");

    // Via the settings form path.
    let (status, body) = put_form(&router, "engines.b.enabled=false", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(json["applied"], serde_json::json!(["engines.b.enabled"]));
    assert_eq!(json["effective_after_restart"], false);

    let meta = search_meta(&router, "two").await;
    assert_eq!(engine_ids(&meta), ["a"], "{meta}");

    let on_disk = std::fs::read_to_string(tmp.path().join("cfg/config.toml")).unwrap();
    assert!(on_disk.contains("enabled = false"), "{on_disk}");

    // Via the dedicated endpoint — same rebuild, reported the same way.
    let (status, _headers, body) = call_json(&router, req("POST", "/api/engines/b/enable")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["effective_after_restart"], false);

    let meta = search_meta(&router, "three").await;
    assert_eq!(engine_ids(&meta), ["a", "b"], "{meta}");
    clear_env();
}

/// `ai.enabled` flips the answer loop live: `/api/answer` stops 503ing
/// on the next request after the save — no restart.
#[tokio::test]
async fn ai_enable_builds_answer_loop_without_restart() {
    let _guard = env_lock().await;
    clear_env();
    let (router, _cfg_tmp, _db_tmp) = app("[ai]\nenabled = false\n");

    let (status, _headers, body) = call_json(
        &router,
        axum::http::Request::builder()
            .method("POST")
            .uri("/api/answer")
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(r#"{"q":"hi"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "ai_disabled");

    // `127.0.0.1:9` refuses instantly — the point is that an answer loop
    // now exists to reach a provider at all, not that it answers.
    let (status, body) = put_form(
        &router,
        "ai.enabled=true&ai.base_url=http%3A%2F%2F127.0.0.1%3A9&ai.api_key=sk-test&ai.model=m",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert!(
        json["applied"]
            .as_array()
            .expect("applied")
            .contains(&serde_json::json!("ai.enabled")),
        "{json}"
    );

    let (status, _headers, body) = call_json(
        &router,
        axum::http::Request::builder()
            .method("POST")
            .uri("/api/answer")
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(r#"{"q":"hi"}"#))
            .unwrap(),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "the rebuilt runtime must hold an answer loop: {body}"
    );
    assert_ne!(body["error"]["code"], "ai_disabled", "{body}");
    clear_env();
}

/// Restart-required keys still save to disk but report as pending.
#[tokio::test]
async fn restart_required_keys_reported_pending() {
    let _guard = env_lock().await;
    clear_env();
    // `[logs]` must already exist for the diff to name the leaf key —
    // a brand-new section reports at table level (`logs`).
    let (router, tmp, _db_tmp) =
        app("[search]\ndeadline_ms = 2000\n\n[logs]\nretention_days = 30\n");

    let (status, body) = put_form(
        &router,
        "logs.retention_days=3&search.deadline_ms=1500",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(
        json["requires_restart"],
        serde_json::json!(["logs.retention_days"])
    );
    assert_eq!(json["applied"], serde_json::json!(["search.deadline_ms"]));
    assert_eq!(json["effective_after_restart"], true);

    // Persistence is not deferred: both values land in the file now.
    let on_disk = std::fs::read_to_string(tmp.path().join("cfg/config.toml")).unwrap();
    assert!(on_disk.contains("retention_days = 3"), "{on_disk}");
    assert!(on_disk.contains("deadline_ms = 1500"), "{on_disk}");

    // And the `config.put` audit row names the changed keys.
    let (status, _headers, rows) = get(&router, "/api/audit?action=config.put").await;
    assert_eq!(status, StatusCode::OK);
    let row = rows
        .as_array()
        .expect("audit rows")
        .iter()
        .find(|r| r["action"].as_str() == Some("config.put"))
        .expect("config.put audit row");
    let changed: Vec<&str> = row["details"]["changed"]
        .as_array()
        .expect("changed list")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(changed.contains(&"logs.retention_days"), "{row}");
    assert!(changed.contains(&"search.deadline_ms"), "{row}");
    clear_env();
}
