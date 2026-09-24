//! W3-05: `/api/stats` exposes the newest `evals/results/*-engines.json`
//! when one exists, and the dashboard's "engine relevance (nightly)"
//! panel renders per-engine domain-hit@5 — or the empty state otherwise.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The dashboard page exists only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use axum::http::StatusCode;
use cauce_engines::ReplayOpts;

mod support;
use support::*;

const REPORT: &str = r#"{
  "kind": "engines",
  "date": "2026-09-24",
  "generated_at": "2026-09-24T04:29:00Z",
  "live": true,
  "engines": [
    {"engine": "bing", "cases": 5, "hits": 4, "domain_hit_at5": 0.8, "threshold": 0.8, "ok": true},
    {"engine": "brave", "cases": 5, "hits": 2, "domain_hit_at5": 0.4, "threshold": 0.8, "ok": false}
  ],
  "below_threshold": ["brave"],
  "outcomes": []
}"#;

#[tokio::test]
async fn stats_and_dashboard_expose_latest_eval_run() {
    let _lock = env_lock().await;
    let (app, _store, _tmp) = app_with(ReplayOpts::default());
    let results = tempfile::tempdir().unwrap();

    // No report on disk yet: the field is absent and the panel shows its
    // empty state (cwd has no evals/results/ during tests).
    // SAFETY: serialized by ENV_LOCK.
    unsafe { std::env::set_var("CAUCE_EVAL_RESULTS_DIR", results.path()) };
    let (status, stats) = get_json(&app, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        stats.get("engine_eval").is_none() || stats["engine_eval"].is_null(),
        "no report file -> no engine_eval: {stats}"
    );
    let (status, body) = get_html(&app, "/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("engine relevance (nightly)"));
    assert!(
        body.contains("no eval run yet"),
        "empty eval state:\n{body}"
    );

    // A report on disk is read per request — stats and dashboard reflect it.
    std::fs::write(results.path().join("2026-09-24-engines.json"), REPORT).unwrap();
    let (status, stats) = get_json(&app, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stats["engine_eval"]["date"], "2026-09-24");
    assert_eq!(stats["engine_eval"]["engines"][0]["engine"], "bing");
    assert_eq!(stats["engine_eval"]["engines"][0]["domain_hit_at5"], 0.8);
    assert_eq!(
        stats["engine_eval"]["below_threshold"],
        serde_json::json!(["brave"])
    );

    let (status, body) = get_html(&app, "/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    for needle in [
        "engine relevance (nightly)",
        "2026-09-24",
        "live",
        "bing",
        "4/5 · 80%",
        "brave",
        "2/5 · 40%",
    ] {
        assert!(body.contains(needle), "missing {needle:?}:\n{body}");
    }

    clear_env();
}
