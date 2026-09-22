//! Golden path end-to-end test (W0-12).
//!
//! Runs the `cauce` binary, starts `cauce serve` with the replay engine, and
//! walks through the full user flow: network search, cache hit, history,
//! stats, click beacon, HTML result page, cache delete, audit, and `cauce trace`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use cauce_core::{CacheKey, ClientKind, SafeSearch, SearchRequest};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::common;

const QUERY: &str = "golden path";
const QUERY_URL: &str = "/api/search?q=golden+path";

#[tokio::test]
async fn replay_golden_path() {
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let server = common::spawn_cauce(&data_dir, &config_dir, "replay").await;
    let addr = server.addr;

    // 1. First search is a network hit.
    let (status, body1) = common::http(addr, "GET", QUERY_URL, None).await;
    assert_eq!(status, 200, "first search failed: {body1}");
    let first: Value = serde_json::from_str(&body1).expect("search JSON");
    assert_eq!(first["meta"]["source"], "network");
    let first_request_id = first["meta"]["request_id"]
        .as_str()
        .expect("request_id")
        .to_string();

    // 2. Same search is a tier-1 cache hit with age_s >= 0.
    let (status, body2) = common::http(addr, "GET", QUERY_URL, None).await;
    assert_eq!(status, 200, "second search failed: {body2}");
    let second: Value = serde_json::from_str(&body2).expect("search JSON");
    assert!(
        second["meta"]["source"].is_object(),
        "second source should be a cache object: {second}"
    );
    assert_eq!(second["meta"]["source"]["cache"]["tier"], 1);
    assert!(
        second["meta"]["source"]["cache"]["age_s"]
            .as_f64()
            .unwrap_or(-1.0)
            >= 0.0
    );

    // 3. History has two rows: network and cache.
    let (status, hist_body) = common::http(addr, "GET", "/api/history?limit=10", None).await;
    assert_eq!(status, 200, "history failed: {hist_body}");
    let history: Vec<Value> = serde_json::from_str(&hist_body).expect("history JSON");
    assert_eq!(history.len(), 2, "expected 2 history rows: {history:?}");
    let sources: Vec<&str> = history
        .iter()
        .filter_map(|h| h["source"].as_str())
        .collect();
    assert!(sources.contains(&"network"));
    assert!(sources.contains(&"cache"));

    // 4. Stats show a 0.5 hit rate after two searches.
    let (status, stats_body) = common::http(addr, "GET", "/api/stats", None).await;
    assert_eq!(status, 200, "stats failed: {stats_body}");
    let stats: Value = serde_json::from_str(&stats_body).expect("stats JSON");
    assert_eq!(stats["searches"], 2);
    assert_eq!(stats["cache_hits"], 1);
    let hit_rate = stats["hit_rate"].as_f64().expect("hit_rate");
    assert!((hit_rate - 0.5).abs() < 1e-9, "hit_rate {hit_rate} != 0.5");

    // 4b. W1-09: stats carries the per-engine metrics row and the admission
    // aggregates.
    assert_eq!(stats["engines"][0]["engine"], "replay", "{stats}");
    assert!(
        stats["engines"][0]["p95_ms"].is_number(),
        "engines[0].p95_ms must be a number: {stats}"
    );
    assert!(stats["admission"]["rejected"].is_number(), "{stats}");

    // 4c. W1-09: /metrics exposes the settled instrument names.
    let (status, metrics_body) = common::http(addr, "GET", "/metrics", None).await;
    assert_eq!(status, 200, "metrics failed: {metrics_body}");
    for name in [
        "cauce_search_requests_total",
        "cauce_engine_requests_total",
        "cauce_engine_breaker_state",
        "cauce_cache_entries",
        "cauce_deadline_hit_total",
    ] {
        assert!(
            metrics_body.contains(name),
            "missing {name}:\n{metrics_body}"
        );
    }

    // Compute the canonical cache key before recording the UI click. This is
    // also the query_hash sent by the real result-link beacon.
    let cache_req = SearchRequest {
        q: QUERY.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: None,
        client: ClientKind::Api,
    };
    let key = CacheKey::from(&cache_req);

    // 5. Click beacon is accepted.
    let click = serde_json::to_string(&json!({
        "url": "https://example.com/golden-result",
        "title": "golden path: a practical handbook with trade-offs",
        "query_hash": key.as_str(),
        "position": 0,
    }))
    .unwrap();
    let (status, _) = common::http(addr, "POST", "/api/click", Some(&click)).await;
    assert_eq!(status, 204, "click should return 204");

    // 6. History now also contains the click.
    let (status, hist2_body) = common::http(addr, "GET", "/api/history?limit=10", None).await;
    assert_eq!(status, 200, "history after click failed: {hist2_body}");
    let history2: Vec<Value> = serde_json::from_str(&hist2_body).expect("history JSON");
    assert!(
        history2.iter().any(|h| h["kind"] == "click"),
        "click not in history"
    );

    // 7. HTML search page is served from cache and contains a replay title.
    // `ui` builds only: `--no-default-features` binaries have no pages.
    if cfg!(feature = "ui") {
        let (status, html) = common::http(addr, "GET", "/search?q=golden+path", None).await;
        assert_eq!(status, 200, "html search failed");
        assert!(
            html.contains("golden path:"),
            "HTML should contain a replay title"
        );
        assert!(html.contains("cached"), "HTML should show cached badge");
    }

    // 8. Delete the exact cache entry; the next search is network again.
    let (status, del_body) = common::http(
        addr,
        "DELETE",
        &format!("/api/cache/{}", key.as_str()),
        None,
    )
    .await;
    assert_eq!(status, 200, "delete cache failed: {del_body}");

    let (status, body3) = common::http(addr, "GET", QUERY_URL, None).await;
    assert_eq!(status, 200, "third search failed: {body3}");
    let third: Value = serde_json::from_str(&body3).expect("search JSON");
    assert_eq!(third["meta"]["source"], "network");

    // 9. Audit contains the cache delete with actor "api".
    let (status, audit_body) = common::http(addr, "GET", "/api/audit?limit=10", None).await;
    assert_eq!(status, 200, "audit failed: {audit_body}");
    let audit: Vec<Value> = serde_json::from_str(&audit_body).expect("audit JSON");
    let delete_row = audit
        .iter()
        .find(|a| a["action"].as_str() == Some("cache.delete"))
        .expect("cache.delete audit row missing");
    assert_eq!(delete_row["actor"], "api");

    // 10. `cauce trace <request_id>` prints the replay engine span. Poll the
    // JSONL writer with a short bounded retry instead of relying on a fixed
    // sleep, since flush latency varies between local and CI runners.
    let mut trace = String::new();
    for _ in 0..40 {
        let trace_out = Command::new(common::cauce_bin())
            .arg("trace")
            .arg(&first_request_id)
            .env("CAUCE_DATA_DIR", &data_dir)
            .env("CAUCE_CONFIG_DIR", &config_dir)
            .output()
            .expect("cauce trace command");
        assert!(
            trace_out.status.success(),
            "cauce trace failed: {}",
            String::from_utf8_lossy(&trace_out.stderr)
        );
        trace = String::from_utf8_lossy(&trace_out.stdout).into_owned();
        if trace.contains("engine") && trace.contains("replay") {
            break;
        }
        sleep(Duration::from_millis(25));
    }
    assert!(
        trace.contains("engine"),
        "trace should contain an engine span"
    );
    assert!(
        trace.contains("replay"),
        "trace should mention the replay engine"
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn live_ddgs_first_two_assertions() {
    if std::env::var("CAUCE_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let server = common::spawn_cauce(&data_dir, &config_dir, "ddgs").await;
    let addr = server.addr;
    let query = "tanstack+router";

    let (status, body1) = common::http(addr, "GET", &format!("/api/search?q={query}"), None).await;
    assert_eq!(status, 200, "live search failed: {body1}");
    let first: Value = serde_json::from_str(&body1).expect("search JSON");
    assert_eq!(first["meta"]["source"], "network");

    let (status, body2) = common::http(addr, "GET", &format!("/api/search?q={query}"), None).await;
    assert_eq!(status, 200, "live second search failed: {body2}");
    let second: Value = serde_json::from_str(&body2).expect("search JSON");
    assert!(
        second["meta"]["source"]["cache"].is_object(),
        "live second search should be cached: {second}"
    );
    assert!(
        second["meta"]["source"]["cache"]["age_s"]
            .as_f64()
            .unwrap_or(-1.0)
            >= 0.0
    );

    server.shutdown().await.expect("shutdown");
}
