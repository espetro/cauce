//! HTTP surface tests (W0-09): the `/api/*` handlers — search,
//! history + click + stats, cache admin, the error envelope, and
//! admission. Plan-table conformance lives in `tests/routes_table.rs`;
//! SSE, the Host/Origin guard and the config API are `tests/{sse,
//! host_guard,config_api}.rs`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use cauce_core::config::Config;
use cauce_core::{Admission, AdmissionLimits, SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::json;
use tower::ServiceExt;

mod support;
use support::*;

// ---------------------------------------------------------------------------
// Handler behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_network_then_cache_hit() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/api/search?q=tanstack%20router").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["meta"]["source"], json!("network"));
    let request_id = body["meta"]["request_id"].as_str().unwrap();
    assert_eq!(
        headers["x-request-id"].to_str().unwrap(),
        request_id,
        "X-Request-Id must equal meta.request_id"
    );
    assert!(body["results"].as_array().unwrap().len() >= 5);

    let (status, headers, body) = get(&router, "/api/search?q=tanstack%20router").await;
    assert_eq!(status, StatusCode::OK);
    let source = &body["meta"]["source"];
    assert_eq!(source["cache"]["tier"], 1, "{body}");
    assert!(source["cache"]["ttl_s"].as_u64().unwrap() > 0);
    assert_eq!(source["cache"]["stale"], false);
    // A second request gets its own id.
    assert_ne!(headers["x-request-id"].to_str().unwrap(), request_id);
}

/// An inbound `X-Request-Id` that parses as a UUID is honoured end to end.
#[tokio::test]
async fn inbound_request_id_is_honoured() {
    let (router, _state, _tmp) = app();
    let id = uuid::Uuid::now_v7();
    let request = Request::builder()
        .method("GET")
        .uri("/api/search?q=request-id")
        .header("x-request-id", id.to_string())
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-request-id"].to_str().unwrap(), id.to_string());
    assert_eq!(body["meta"]["request_id"], id.to_string());
}

#[tokio::test]
async fn history_click_and_stats() {
    let (router, _state, _tmp) = app();

    // One network search + one cache hit = two history rows, hit rate 0.5.
    // The first request uses different casing: the normalized query is
    // identical, so the second request still hits the cache, and history
    // keeps both the normalized `query` and the submitted `query_raw` (#89).
    get(&router, "/api/search?q=History-CHECK").await;
    get(&router, "/api/search?q=history-check").await;

    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{body}");
    assert_eq!(rows[0]["kind"], "search");
    assert_eq!(rows[0]["query"], "history-check");
    assert_eq!(rows[0]["query_raw"], "history-check");
    assert_eq!(rows[1]["query"], "history-check");
    assert_eq!(rows[1]["query_raw"], "History-CHECK");

    // History merges searches and clicks by (ts DESC, id DESC) at
    // millisecond precision; without a pause the click can share the last
    // search's ms and lose the id tiebreak, flipping rows[0].
    tokio::time::sleep(Duration::from_millis(2)).await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/click")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"url":"https://example.com/a","title":"A","position":0}"#,
        ))
        .unwrap();
    let (status, _, _) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["kind"], "click", "{body}");

    let (status, _, body) = get(&router, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["searches"], 2);
    assert_eq!(body["cache_hits"], 1);
    assert_eq!(body["hit_rate"], 0.5);
}

/// Acceptance: `DELETE /api/cache/{key}` then the same search is `network`
/// again, and the delete lands in `audit` with actor `api`.
#[tokio::test]
async fn cache_admin_delete_and_audit() {
    let (router, _state, _tmp) = app();

    let (_, _, body) = get(&router, "/api/search?q=cache-me").await;
    assert_eq!(body["meta"]["source"], json!("network"));

    // The entry is listed and addressable by its hex key.
    let (status, _, body) = get(&router, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 1, "{body}");
    let key = entries[0]["key"].as_str().unwrap().to_string();
    assert_eq!(key.len(), 64);

    let (status, _, body) = get(&router, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["key"], key);

    // Delete it: audited, and the same query hits the network again.
    let request = req("DELETE", &format!("/api/cache/{key}"));
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);

    let (status, _, _) = get(&router, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, body) = get(&router, "/api/search?q=cache-me").await;
    assert_eq!(body["meta"]["source"], json!("network"), "{body}");

    let (status, _, body) = get(&router, "/api/audit").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["action"] == "cache.delete" && r["actor"] == "api" && r["target"] == key),
        "audit rows: {body}"
    );
}

/// `DELETE /api/cache` bulk semantics: exactly one of `expired` / `all`.
#[tokio::test]
async fn cache_bulk_delete_flags() {
    let (router, _state, _tmp) = app();
    get(&router, "/api/search?q=bulk-a").await;
    get(&router, "/api/search?q=bulk-b").await;

    // Neither flag -> 400; both -> 400.
    for uri in ["/api/cache", "/api/cache?expired=true&all=true"] {
        let (status, _, body) = call_json(&router, req("DELETE", uri)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_envelope(&body, "bad_request");
    }

    // expired=true evicts only expired rows (none here) but still audits.
    let (status, _, body) = call_json(&router, req("DELETE", "/api/cache?expired=true")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], 0);

    let (status, _, body) = call_json(&router, req("DELETE", "/api/cache?all=true")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], 2);

    let (_, _, body) = get(&router, "/api/audit").await;
    let rows = body.as_array().unwrap();
    for action in ["cache.evict_expired", "cache.clear"] {
        assert!(
            rows.iter()
                .any(|r| r["action"] == action && r["actor"] == "api"),
            "missing audit row for {action}: {body}"
        );
    }
}

#[tokio::test]
async fn error_envelope_and_param_validation() {
    let (router, _state, _tmp) = app();

    // Missing/blank q, unknown param, bad values -> 400 envelope. A
    // whitespace-only q must be rejected *before* the pipeline fans out
    // on the empty normalized query (#89).
    for uri in [
        "/api/search",
        "/api/search?q=",
        "/api/search?q=%20%09",
        "/api/search?q=%20%20%20",
        "/api/search?q=x&bogus=1",
        "/api/search?q=x&safesearch=9",
        "/api/search?q=x&page=0",
        "/api/search?q=x&page=abc",
        "/api/search?q=x&q=y",
        "/api/history?since=not-a-date",
        "/api/cache/not-hex-at-all",
    ] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_envelope(&body, "bad_request");
    }

    // No rejected request reached the pipeline: nothing was logged.
    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0, "{body}");

    // Malformed and absent cache keys.
    let (status, _, body) = get(&router, &format!("/api/cache/{}", "f".repeat(64))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_envelope(&body, "not_found");

    // Wrong method on a mounted path -> 405 envelope.
    let (status, _, body) = call_json(&router, req("DELETE", "/api/search")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_envelope(&body, "method_not_allowed");

    // Bad click JSON -> 400 envelope.
    let request = Request::builder()
        .method("POST")
        .uri("/api/click")
        .body(Body::from("{nope"))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");
}

/// #85: a click beacon with a malformed `query_hash` is rejected with a
/// 400 instead of storing a value that never joins to `search_log`.
#[tokio::test]
async fn click_beacon_rejects_malformed_query_hash() {
    let (router, _state, _tmp) = app();
    let request = Request::builder()
        .method("POST")
        .uri("/api/click")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"url":"https://example.com/a","query_hash":"garbage"}"#,
        ))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "bad_request");

    // Nothing was stored.
    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0, "{body}");
}

/// `NoEngines`/`UnknownEngines` mapping: a pin naming any id outside the
/// configured set is 400 `unknown_engines` naming the rejected ids and the
/// configured set (issue #90 strict contract — partial pins no longer
/// truncate), even with zero configured engines; an empty/zero configured
/// set with no pin is 503 `no_engines`.
#[tokio::test]
async fn no_engines_status_mapping() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = get(&router, "/api/search?q=x&engines=nosuch").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "unknown_engines");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("nosuch"),
        "message names the rejected id: {message}"
    );
    assert!(
        message.contains("replay"),
        "message lists the configured set: {message}"
    );

    // A partially-valid pin rejects the whole request (issue #90): no
    // silent truncation to the known ids.
    let (status, _, body) = get(&router, "/api/search?q=x&engines=replay,nosuch").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "unknown_engines");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("nosuch"), "{message}");
    assert!(message.contains("replay"), "{message}");

    // A valid pin on a configured engine runs (and keys the cache entry
    // separately from the unpinned search).
    let (status, _, body) = get(&router, "/api/search?q=pinned&engines=replay").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let tmp = tempfile::tempdir().unwrap();
    let store =
        Arc::new(SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).unwrap());
    let pipeline = Arc::new(SearchPipeline::new(store.clone(), vec![]));
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    // Non-empty pin with zero configured engines is still the caller's
    // error — every pin id is unknown when nothing is configured.
    let (status, _, body) = get(&router, "/api/search?q=x&engines=replay").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "unknown_engines");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("replay"), "{message}");

    // No pin with zero configured engines is the operator's error.
    let (status, _, body) = get(&router, "/api/search?q=x").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_envelope(&body, "no_engines");
}

#[tokio::test]
async fn health_reports_ok() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = get(&router, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

/// W1-07 acceptance: `PipelineError::RateLimited` maps to 429 with a
/// `Retry-After` header and the `rate_limited` envelope code.
#[tokio::test]
async fn search_queue_overflow_returns_429_retry_after() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let engine = Arc::new(Replay::new(ReplayOpts {
        latency_ms: 300,
        ..ReplayOpts::default()
    }));
    let pipeline = Arc::new(
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_admission(Admission::new(
            AdmissionLimits {
                max_wait: Duration::from_millis(1),
                max_concurrent_per_engine: 1,
            },
        )),
    );
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    // Occupy the single engine slot with an in-flight request.
    let holder = tokio::spawn({
        let router = router.clone();
        async move { router.oneshot(req("GET", "/api/search?q=holder")).await }
    });
    // `call_count` ticks at the top of `search`: 1 means the permit is held.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while engine.call_count() == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(engine.call_count(), 1, "holder never reached the engine");

    let (status, headers, body) = get(&router, "/api/search?q=overflow").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(headers[header::RETRY_AFTER], "1");
    assert_envelope(&body, "rate_limited");

    let holder_resp = holder.await.unwrap().expect("holder response");
    assert_eq!(holder_resp.status(), StatusCode::OK);
}
