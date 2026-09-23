//! `/dashboard` page tests (W2-03 + the 2026-09-22 `outcome` amendment).
//!
//! One test, one registry: the metrics registry is process-global, so this
//! file holds a single `#[tokio::test]` whose sequential steps assert the
//! empty state, the populated page and the `outcome="error"` split without
//! cross-test bleed.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::SearchPipeline;
use cauce_core::StoreTuning;
use cauce_core::config::Config;
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

fn app_with(opts: ReplayOpts) -> (Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(opts))],
    ));
    (
        build_router(AppState::new(pipeline, store, Config::default())),
        tmp,
    )
}

async fn call(router: &Router, uri: &str, accept: &str) -> (StatusCode, String) {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .header("Accept", accept)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    call(router, uri, "text/html").await
}

/// W2-03 acceptance, end to end on replay: empty-DB placeholders, mixed
/// traffic -> non-zero hit rate + engine rows, window selector, and the
/// amendment's `outcome="error"` increment on a forced 502.
#[tokio::test]
async fn dashboard_empty_populated_and_error_outcome() {
    let (app, _tmp) = app_with(ReplayOpts::default());

    // ---- empty DB: every panel renders its "no data yet" state -----------
    let (status, body) = get_html(&app, "/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("no search log data yet"),
        "empty dashboard should show the muted placeholders:\n{body}"
    );
    assert!(
        body.contains("no engine data yet"),
        "empty dashboard should show the engine placeholder:\n{body}"
    );
    assert!(
        body.contains("href=\"/dashboard?days=7\"") && body.contains("href=\"/dashboard?days=30\""),
        "window selector links:\n{body}"
    );
    assert!(
        body.contains("class=\"request-id\""),
        "footer carries the copyable request id:\n{body}"
    );

    // ---- mixed replay traffic: network + tier-1 cache hits ----------------
    for uri in [
        "/api/search?q=dashalpha",
        "/api/search?q=dashalpha",
        "/api/search?q=dashbeta",
        "/api/search?q=dashbeta",
    ] {
        let (status, _) = call(&app, uri, "application/json").await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }

    let (status, body) = get_html(&app, "/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<svg") && body.contains("<rect"),
        "searches-per-day renders inline SVG bars:\n{body}"
    );
    assert!(
        body.contains("50%"),
        "2 cache hits / 4 searches should render 50%:\n{body}"
    );
    assert!(body.contains("tier 1"), "hit-rate-by-tier row:\n{body}");
    assert!(body.contains("dashalpha"), "top queries list:\n{body}");
    assert!(body.contains(">api<"), "client split:\n{body}");
    assert!(body.contains(">ok<"), "outcome split:\n{body}");
    assert!(body.contains("p50"), "latency percentiles:\n{body}");
    assert!(
        body.contains(">replay<") && body.contains("href=\"/engines\""),
        "engine table links to /engines:\n{body}"
    );
    assert!(
        body.contains("@media (max-width: 639px)")
            && body.contains("data-label=\"http ms (med/p80/p95)\""),
        "engine metrics use labeled mobile cells without horizontal overflow:\n{body}"
    );
    assert!(
        body.contains("closed"),
        "engine row shows breaker state:\n{body}"
    );
    assert!(
        body.contains("db size"),
        "cache panel shows the db size:\n{body}"
    );

    // The window selector is live: 30 days renders the same data.
    let (status, _) = get_html(&app, "/dashboard?days=30").await;
    assert_eq!(status, StatusCode::OK);

    // Content negotiation: `Accept: application/json` on the page route
    // returns the same StatsSnapshot `/api/stats` serves.
    let (status, body) = call(&app, "/dashboard", "application/json").await;
    assert_eq!(status, StatusCode::OK);
    let snap: Value = serde_json::from_str(&body).expect("stats json");
    assert_eq!(snap["searches"], 4, "{snap}");
    assert_eq!(snap["cache_hits"], 2, "{snap}");
    assert!(snap["top_queries"].is_array(), "{snap}");
    assert!(snap["hits_by_tier"].is_array(), "{snap}");
    assert!(snap["cache_db_bytes"].is_number(), "{snap}");
    assert_eq!(snap["outcomes"]["ok"], 4, "{snap}");

    // ---- forced 502 -> cauce_search_requests_total{outcome="error"} -------
    let (failing, _tmp2) = app_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });
    let (status, _) = call(&failing, "/api/search?q=boom", "application/json").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "blocked replay -> 502");

    let (status, metrics) = call(&app, "/metrics", "text/plain").await;
    assert_eq!(status, StatusCode::OK);
    let error_line = metrics
        .lines()
        .find(|l| l.starts_with("cauce_search_requests_total{") && l.contains("outcome=\"error\""));
    let error_line =
        error_line.unwrap_or_else(|| panic!("missing outcome=error series:\n{metrics}"));
    let value: u64 = error_line
        .rsplit(' ')
        .next()
        .and_then(|v| v.parse().ok())
        .expect("counter value");
    assert!(value >= 1, "error outcome incremented: {error_line}");

    // The dashboard surfaces the error/rejected split.
    let (status, body) = get_html(&failing, "/dashboard").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(">error<"),
        "outcomes panel shows the error row:\n{body}"
    );
}
