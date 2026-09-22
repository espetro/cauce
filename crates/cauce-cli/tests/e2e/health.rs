//! Engine health end-to-end (W1-06).
//!
//! A `blocked` replay engine opens the breaker on the first call, is
//! skipped on the next, and the `Open` state survives a SIGKILL restart
//! because the breaker transition is persisted urgently (the
//! `engine_health` row is on disk before the tripping response returns).
//! `POST /api/engines/{id}/reset` re-admits the engine.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde_json::Value;
use tempfile::TempDir;

use crate::common;

const BLOCKED: &[(&str, &str)] = &[("CAUCE_REPLAY_BLOCKED", "1")];

/// Start `cauce serve` with a blocked replay engine on the given data dir.
async fn spawn_blocked(data_dir: &TempDir, config_dir: &TempDir) -> common::ServerGuard {
    common::spawn_cauce_env(data_dir.path(), config_dir.path(), "replay", BLOCKED).await
}

async fn engine_row(addr: std::net::SocketAddr) -> Value {
    let (status, body) = common::http(addr, "GET", "/api/engines", None).await;
    assert_eq!(status, 200, "GET /api/engines failed: {body}");
    let rows: Vec<Value> = serde_json::from_str(&body).expect("engines JSON");
    rows.into_iter()
        .find(|r| r["engine"] == "replay")
        .expect("replay engine row missing")
}

#[tokio::test]
async fn blocked_replay_opens_skips_and_survives_restart() {
    let data_dir = TempDir::new().expect("data dir");
    let config_dir = TempDir::new().expect("config dir");

    let server = spawn_blocked(&data_dir, &config_dir).await;
    let addr = server.addr;

    // First call reaches the engine, fails `Blocked`, opens the breaker.
    let (status, body) = common::http(addr, "GET", "/api/search?q=one", None).await;
    assert_eq!(status, 502, "first search should hit the engine: {body}");

    let row = engine_row(addr).await;
    assert_eq!(row["breaker"], "open", "{row}");
    assert_eq!(row["failures"], 1);
    assert!(row["breaker_until"].is_string(), "{row}");

    // The open engine is skipped, not called: 503 `breaker_open`.
    let (status, body) = common::http(addr, "GET", "/api/search?q=two", None).await;
    assert_eq!(status, 503, "open breaker should skip: {body}");
    let err: Value = serde_json::from_str(&body).expect("error JSON");
    assert_eq!(err["error"]["code"], "breaker_open");

    // SIGKILL the server: only urgently-persisted state survives. The
    // breaker transition flushed before the 502 response above, so the
    // `engine_health` row is already in the temp DB.
    server.shutdown().await.expect("shutdown");

    // Restart against the same data dir: the breaker is still Open and
    // the engine is skipped without being called.
    let server = spawn_blocked(&data_dir, &config_dir).await;
    let addr = server.addr;

    let row = engine_row(addr).await;
    assert_eq!(
        row["breaker"], "open",
        "breaker state should persist across restart: {row}"
    );
    assert_eq!(row["failures"], 1, "{row}");

    let (status, body) = common::http(addr, "GET", "/api/search?q=three", None).await;
    assert_eq!(status, 503, "restarted breaker should still skip: {body}");
    let err: Value = serde_json::from_str(&body).expect("error JSON");
    assert_eq!(err["error"]["code"], "breaker_open");

    // Reset re-admits the engine; it fails again (still blocked) and the
    // breaker re-opens.
    let (status, body) = common::http(addr, "POST", "/api/engines/replay/reset", None).await;
    assert_eq!(status, 200, "reset failed: {body}");
    let row: Value = serde_json::from_str(&body).expect("reset JSON");
    assert_eq!(row["breaker"], "closed", "{row}");

    let (status, body) = common::http(addr, "GET", "/api/search?q=four", None).await;
    assert_eq!(status, 502, "post-reset search should reach engine: {body}");

    let row = engine_row(addr).await;
    assert_eq!(row["breaker"], "open", "{row}");

    // The reset itself is audited.
    let (status, body) = common::http(addr, "GET", "/api/audit?limit=20", None).await;
    assert_eq!(status, 200, "audit failed: {body}");
    let audit: Vec<Value> = serde_json::from_str(&body).expect("audit JSON");
    assert!(
        audit
            .iter()
            .any(|a| a["action"] == "engine.reset" && a["target"] == "replay"),
        "engine.reset audit row missing: {body}"
    );

    server.shutdown().await.expect("shutdown");
}
