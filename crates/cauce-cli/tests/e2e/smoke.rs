//! Live engine smoke test (W1-11).
//!
//! Nightly canary for the real upstreams: `cauce engine test <engine>
//! --live "<query>"` for three queries each against bing, brave and
//! wikipedia. Each call must exit 0 and print at least 3 parsed results;
//! a `Blocked`/`RateLimited` engine error exits 1 with `parse: ...` on
//! stderr, so the exit status is the no-block assertion.
//!
//! Asserts on count, not content: a poisoned SERP (HTTP 200 with junk
//! markup) still yields parsed rows — the canary catches hard failures
//! (blocks, rate limits, selector drift to zero results), not result
//! relevance.
//!
//! Only runs when `CAUCE_LIVE=1` (the nightly workflow sets it).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

use serde_json::Value;
use tempfile::TempDir;

use crate::common;

const ENGINES: &[&str] = &["bing", "brave", "wikipedia"];
// Broad single-term queries: the wikipedia engine is an OpenSearch
// title-prefix matcher, so multi-word queries can legitimately return
// fewer than 3 hits.
const QUERIES: &[&str] = &["rust", "python", "mercury"];

/// One `cauce engine test <engine> --live "<query>"` invocation, isolated
/// from the developer's real config/data dirs.
fn live_query(config_dir: &TempDir, data_dir: &TempDir, engine: &str, query: &str) -> Output {
    Command::new(common::cauce_bin())
        .current_dir(common::workspace_root())
        .args(["engine", "test", engine, "--live", query])
        .env("CAUCE_CONFIG_DIR", config_dir.path())
        .env("CAUCE_DATA_DIR", data_dir.path())
        .output()
        .expect("spawn cauce engine test")
}

#[test]
fn live_engine_smoke() {
    if std::env::var("CAUCE_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skipping live engine smoke: set CAUCE_LIVE=1 to run it");
        return;
    }
    let config_dir = TempDir::new().expect("config dir");
    let data_dir = TempDir::new().expect("data dir");

    let mut failures: Vec<String> = Vec::new();
    for &engine in ENGINES {
        for &query in QUERIES {
            // One retry with a short backoff: upstreams rate-limit
            // transiently and the nightly should not flake on it.
            let mut out = live_query(&config_dir, &data_dir, engine, query);
            if !out.status.success() {
                sleep(Duration::from_secs(5));
                out = live_query(&config_dir, &data_dir, engine, query);
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                failures.push(format!(
                    "{engine} {query:?}: exit {} — {}",
                    out.status,
                    stderr.trim()
                ));
                continue;
            }
            let results: Vec<Value> = match serde_json::from_str(&stdout) {
                Ok(results) => results,
                Err(e) => {
                    failures.push(format!(
                        "{engine} {query:?}: stdout is not a JSON array: {e}: {stdout}"
                    ));
                    continue;
                }
            };
            if results.len() < 3 {
                failures.push(format!(
                    "{engine} {query:?}: {} results (< 3)",
                    results.len()
                ));
            }
        }
        // Politeness between engines: the fetch itself is already
        // token-bucketed, but the canary is in no hurry.
        sleep(Duration::from_millis(500));
    }
    assert!(
        failures.is_empty(),
        "live engine smoke failures:\n{}",
        failures.join("\n")
    );
}
