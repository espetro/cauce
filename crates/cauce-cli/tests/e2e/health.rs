//! Engine health end-to-end (W1-06).
//!
//! A `blocked` replay engine opens the breaker on the first call, is
//! skipped on the next, and the `Open` state survives a SIGKILL restart
//! because the breaker transition is persisted urgently (the
//! `engine_health` row is on disk before the tripping response returns).
//! `POST /api/engines/{id}/reset` re-admits the engine. Issue #227
//! adds the self-healing side: a background prober recovers the engine
//! without traffic or a reset.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

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
    assert_eq!(row["breaker"], "half_open", "{row}");

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

/// Tiny breaker/probe windows so the recovery cycle runs in seconds:
/// every replay call fails `Transport` (`fail_every=1`),
/// `degraded_threshold=1` opens on the first call for
/// `degraded_window_s=3`, and the prober re-tries on the adaptive
/// `probe_window_s=2 -> probe_window_max_s=4` backoff every
/// `probe_tick_s=1`.
const FAST_BREAKER: &[(&str, &str)] = &[
    ("CAUCE_HEALTH_DEGRADED_THRESHOLD", "1"),
    ("CAUCE_HEALTH_DEGRADED_WINDOW_S", "3"),
    ("CAUCE_HEALTH_PROBE_WINDOW_S", "2"),
    ("CAUCE_HEALTH_PROBE_WINDOW_MAX_S", "4"),
    ("CAUCE_HEALTH_PROBE_TICK_S", "1"),
];

/// Issue #227: self-healing recovery without a manual reset. Phase 1
/// faults every replay call (`fail_every=1` -> `Transport`): the
/// breaker opens on `degraded_window_s` and the prober keeps it
/// honestly `Open` — `failures` grows at the bounded probe rate and
/// `next_probe_at` is exposed. Phase 2 restarts the same data dir
/// WITHOUT the fault env: the persisted `Open` row is loaded and the
/// prober closes it in bounded time — zero searches, zero
/// `POST /reset` calls.
#[tokio::test]
async fn prober_recovers_engine_across_restart_without_reset() {
    let data_dir = TempDir::new().expect("data dir");
    let config_dir = TempDir::new().expect("config dir");

    let faulted: Vec<(&str, &str)> = FAST_BREAKER
        .iter()
        .copied()
        .chain([("CAUCE_REPLAY_FAIL_EVERY", "1")])
        .collect();
    let server =
        common::spawn_cauce_env(data_dir.path(), config_dir.path(), "replay", &faulted).await;
    let addr = server.addr;

    // One call opens the breaker (Transport feeds the degraded streak).
    let (status, body) = common::http(addr, "GET", "/api/search?q=one", None).await;
    assert_eq!(status, 502, "first search should hit the engine: {body}");
    let row = engine_row(addr).await;
    assert_eq!(row["breaker"], "open", "{row}");
    assert!(
        row["next_probe_at"].is_string(),
        "next-probe ETA must be exposed while open: {row}"
    );
    assert!(row["last_error"].is_string(), "{row}");

    // While the engine stays dead the prober keeps it honestly open.
    // `failures` grows — the prober is running — but the retry cadence
    // stays inside the adaptive window, not hammered per tick.
    let failures_t0 = row["failures"].as_u64().unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;
    let row = engine_row(addr).await;
    assert_eq!(
        row["breaker"], "open",
        "dead engine stays honestly open: {row}"
    );
    let failures_t6 = row["failures"].as_u64().unwrap();
    assert!(
        failures_t6 > failures_t0,
        "prober attempts keep counting failures: {failures_t0} -> {failures_t6}"
    );
    assert!(
        failures_t6 - failures_t0 <= 5,
        "dead engine must not be hammered: +{} calls in 6 s",
        failures_t6 - failures_t0
    );

    server.shutdown().await.expect("shutdown");

    // Recovery: same data dir, no fault env. The persisted `Open` row
    // is loaded, the prober claims its probe within a couple of
    // windows, and the breaker closes on its own — no search ever
    // gates the engine, and nobody calls `POST /reset`.
    let server =
        common::spawn_cauce_env(data_dir.path(), config_dir.path(), "replay", FAST_BREAKER).await;
    let addr = server.addr;

    let mut closed = None;
    for _ in 0..30 {
        let row = engine_row(addr).await;
        if row["breaker"] == "closed" {
            closed = Some(row);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let row =
        closed.expect("breaker should close without a search or reset within ~15 s of restart");
    assert!(
        row["next_probe_at"].is_null(),
        "closed row has no pending probe: {row}"
    );
    assert!(row["last_ok_at"].is_string(), "{row}");

    // And the recovered engine serves the next real search.
    let (status, body) = common::http(addr, "GET", "/api/search?q=recovered", None).await;
    assert_eq!(status, 200, "recovered engine should serve: {body}");

    server.shutdown().await.expect("shutdown");
}
