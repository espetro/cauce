//! W1-09 acceptance: after one replay search, `GET /metrics` returns
//! Prometheus text that a real parser accepts and that carries every settled
//! instrument, and `/api/stats` carries `engines[]` with numeric percentile
//! fields.
//!
//! This file deliberately holds a single test: the metrics registry is
//! process-global, so two tests in one binary would share its series.
//! One test, one registry, no bleed.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use cauce_server::{METRICS_CONTENT_TYPE, build_router};

mod support;
use support::*;

#[tokio::test]
async fn metrics_endpoint_and_stats_after_replay_search() {
    let (state, _tmp) = test_state();
    let router = build_router(state);

    // One replay (network) search populates the search/engine instruments.
    let (status, _) = get_json(&router, "/api/search?q=metrics-check").await;
    assert_eq!(status, StatusCode::OK);

    // ---- GET /metrics: parseable Prometheus text --------------------------
    let (status, headers, text) = get_headers(&router, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some(METRICS_CONTENT_TYPE),
        "Prometheus content type"
    );

    let scrape = prometheus_parse::Scrape::parse(
        text.lines()
            .map(|line| Ok::<_, std::io::Error>(line.to_string())),
    )
    .expect("prometheus text exposition parses");
    let names: BTreeSet<&str> = scrape.samples.iter().map(|s| s.metric.as_str()).collect();

    // Counters and gauges carry the exact settled name; histograms emit
    // `<name>_bucket`/`_sum`/`_count` series.
    for name in [
        "cauce_search_requests_total",
        "cauce_engine_requests_total",
        "cauce_engine_breaker_state",
        "cauce_cache_entries",
        "cauce_admission_rejected_total",
        "cauce_deadline_hit_total",
        "cauce_stale_served_total",
    ] {
        assert!(names.contains(name), "missing metric {name}:\n{text}");
    }
    for name in [
        "cauce_search_duration_ms",
        "cauce_ttfr_ms",
        "cauce_engine_duration_ms",
        "cauce_engine_results",
        "cauce_admission_wait_ms",
    ] {
        assert!(
            names.iter().any(|m| m.starts_with(&format!("{name}_"))),
            "missing histogram {name}:\n{text}"
        );
    }

    // Label contracts from the issue.
    let has = |metric: &str, label: &str| {
        scrape
            .samples
            .iter()
            .any(|s| s.metric.starts_with(metric) && s.labels.get(label).is_some())
    };
    assert!(has("cauce_search_requests_total", "client"));
    assert!(has("cauce_search_requests_total", "source"));
    assert!(has("cauce_search_requests_total", "tier"));
    assert!(has("cauce_search_requests_total", "outcome"));
    assert!(has("cauce_search_duration_ms", "source"));
    assert!(has("cauce_engine_requests_total", "engine"));
    assert!(has("cauce_engine_requests_total", "outcome"));
    assert!(has("cauce_engine_duration_ms", "engine"));
    assert!(has("cauce_engine_duration_ms", "phase"));
    assert!(has("cauce_engine_results", "engine"));
    assert!(has("cauce_engine_breaker_state", "engine"));
    // The replay engine performs both phases.
    for phase in ["http", "parse"] {
        assert!(
            scrape
                .samples
                .iter()
                .any(|s| s.metric.starts_with("cauce_engine_duration_ms")
                    && s.labels.get("phase") == Some(phase)),
            "missing cauce_engine_duration_ms{{phase={phase}}}:\n{text}"
        );
    }

    // ---- GET /api/stats: engines[] + admission aggregates ------------------
    let (status, body) = get_json(&router, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);

    let engines = body["engines"].as_array().expect("engines array");
    assert_eq!(engines.len(), 1, "one engine ran: {body}");
    assert_eq!(engines[0]["engine"], "replay");
    assert!(engines[0]["p95_ms"].is_number(), "p95_ms: {body}");
    assert!(engines[0]["median_ms"].is_number());
    assert!(engines[0]["p80_ms"].is_number());
    assert!(engines[0]["reliability_pct"].is_number());
    assert!(engines[0]["result_count"].is_number());
    assert!(engines[0]["http"]["p95_ms"].is_number(), "http p95: {body}");
    assert!(
        engines[0]["parse"]["p95_ms"].is_number(),
        "parse p95: {body}"
    );
    assert!(body["admission"]["rejected"].is_number());
    assert!(body["admission"]["wait_median_ms"].is_number());
}
