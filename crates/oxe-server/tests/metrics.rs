//! W1-09 acceptance: after one replay search, `GET /metrics` returns
//! Prometheus text that a real parser accepts and that carries every settled
//! instrument, and `/api/stats` carries `engines[]` with numeric percentile
//! fields.
//!
//! This file deliberately holds a single test: `AppState` installs the
//! process-global meter provider, so two tests in one binary could race the
//! lazy binding in `Metrics::default()`. One test, one provider, no race.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use oxe_core::config::Config;
use oxe_core::{SearchPipeline, StoreTuning};
use oxe_engines::{Replay, ReplayOpts};
use oxe_server::{AppState, METRICS_CONTENT_TYPE, build_router};
use oxe_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn metrics_endpoint_and_stats_after_replay_search() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    // One replay (network) search populates the search/engine instruments.
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search?q=metrics-check"))
        .await
        .expect("search response");
    assert_eq!(resp.status(), StatusCode::OK);

    // ---- GET /metrics: parseable Prometheus text --------------------------
    let resp = router
        .clone()
        .oneshot(req("GET", "/metrics"))
        .await
        .expect("metrics response");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some(METRICS_CONTENT_TYPE),
        "Prometheus content type"
    );
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).expect("utf8 exposition");

    let scrape = prometheus_parse::Scrape::parse(
        text.lines()
            .map(|line| Ok::<_, std::io::Error>(line.to_string())),
    )
    .expect("prometheus text exposition parses");
    let names: BTreeSet<&str> = scrape.samples.iter().map(|s| s.metric.as_str()).collect();

    // Counters and gauges carry the exact settled name; histograms emit
    // `<name>_bucket`/`_sum`/`_count` series.
    for name in [
        "oxe_search_requests_total",
        "oxe_engine_requests_total",
        "oxe_engine_breaker_state",
        "oxe_cache_entries",
        "oxe_admission_rejected_total",
        "oxe_deadline_hit_total",
        "oxe_stale_served_total",
    ] {
        assert!(names.contains(name), "missing metric {name}:\n{text}");
    }
    for name in [
        "oxe_search_duration_ms",
        "oxe_ttfr_ms",
        "oxe_engine_duration_ms",
        "oxe_engine_results",
        "oxe_admission_wait_ms",
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
    assert!(has("oxe_search_requests_total", "client"));
    assert!(has("oxe_search_requests_total", "source"));
    assert!(has("oxe_search_requests_total", "tier"));
    assert!(has("oxe_search_duration_ms", "source"));
    assert!(has("oxe_engine_requests_total", "engine"));
    assert!(has("oxe_engine_requests_total", "outcome"));
    assert!(has("oxe_engine_duration_ms", "engine"));
    assert!(has("oxe_engine_duration_ms", "phase"));
    assert!(has("oxe_engine_results", "engine"));
    assert!(has("oxe_engine_breaker_state", "engine"));
    // The replay engine performs both phases.
    for phase in ["http", "parse"] {
        assert!(
            scrape
                .samples
                .iter()
                .any(|s| s.metric.starts_with("oxe_engine_duration_ms")
                    && s.labels.get("phase") == Some(phase)),
            "missing oxe_engine_duration_ms{{phase={phase}}}:\n{text}"
        );
    }

    // ---- GET /api/stats: engines[] + admission aggregates ------------------
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/stats"))
        .await
        .expect("stats response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).expect("stats json");

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
