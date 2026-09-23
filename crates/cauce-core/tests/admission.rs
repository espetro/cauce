//! Acceptance tests for W1-07 admission control (parent plan 6.2):
//! singleflight on `CacheKey`, the bounded per-engine queue with
//! `max_wait`, stale-serve on overflow with a background refresh, and
//! `PipelineError::RateLimited` when there is nothing to fall back on.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cauce_core::{
    Admission, AdmissionLimits, CacheKey, EngineId, EngineReport, EngineStatus, PipelineError,
    SearchMeta, SearchPipeline, SearchResponse, SearchResult, Source, Tier,
};
use chrono::Utc;
use support::{StubStore, replay_at, req};
use url::Url;
use uuid::Uuid;

/// Poll `cond` until it holds or `timeout` elapses (engine counters are
/// only observable once the spawned flight task has been scheduled).
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    cond()
}

/// Acceptance: 20 concurrent identical requests against a counting
/// replay engine produce exactly 1 upstream call and 20 responses.
#[tokio::test]
async fn singleflight_dedupes_concurrent_identical_requests() {
    let dir = tempfile::tempdir().unwrap();
    // A small engine delay keeps the flight in the air while all twenty
    // followers register; the counter then proves dedupe.
    let engine = Arc::new(replay_at(dir.path(), |o| o.latency_ms = 20));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);

    let mut handles = Vec::new();
    for _ in 0..20 {
        let pipe = pipe.clone();
        handles.push(tokio::spawn(
            async move { pipe.search(&req("identical")).await },
        ));
    }
    let mut request_ids = std::collections::BTreeSet::new();
    for h in handles {
        let resp = h.await.unwrap().expect("follower response");
        assert!(matches!(resp.meta.source, Source::Network));
        assert_eq!(resp.results.len(), 10);
        request_ids.insert(resp.meta.request_id);
    }

    assert_eq!(
        engine.call_count(),
        1,
        "20 identical requests must collapse into one upstream call"
    );
    assert_eq!(
        request_ids.len(),
        20,
        "every waiter keeps its own request id"
    );
    assert_eq!(
        store.puts.lock().unwrap().len(),
        1,
        "one cache write per flight"
    );
    assert_eq!(
        store.logs.lock().unwrap().len(),
        20,
        "the unconditional search_log write is per request"
    );
}

/// Acceptance: with `max_wait = 1 ms` and a slow engine holding all three
/// per-engine slots, the 4th request overflows with nothing stored and is
/// rejected with `RateLimited` (HTTP maps this to 429 + `Retry-After`).
#[tokio::test]
async fn queue_overflow_returns_rate_limited() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |o| o.latency_ms = 300));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]).with_admission(
        Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(1),
            max_concurrent_per_engine: 3,
        }),
    );

    // Distinct queries: each elects its own leader and holds one of the
    // three engine permits for the engine's 300 ms latency.
    let mut holders = Vec::new();
    for i in 0..3 {
        let pipe = pipe.clone();
        holders.push(tokio::spawn(async move {
            pipe.search(&req(&format!("holder-{i}"))).await
        }));
    }
    // `call_count` increments at the top of `search`, so 3 calls means all
    // three permits are held.
    assert!(
        wait_until(Duration::from_secs(2), || engine.call_count() == 3).await,
        "holders never acquired their engine permits"
    );

    let err = pipe
        .search(&req("overflow"))
        .await
        .expect_err("saturated queue must reject");
    assert!(
        matches!(err, PipelineError::RateLimited { retry_after_s: 1 }),
        "expected RateLimited{{retry_after_s: 1}}, got {err:?}"
    );

    for h in holders {
        h.await.unwrap().expect("holder response");
    }
    assert_eq!(engine.call_count(), 3, "the rejected request ran nothing");
}

/// Acceptance: same overflow, but an expired row exists for the key —
/// the request is served `Source::Cache { stale: true }` and a background
/// refresh re-fetches once a slot frees.
#[tokio::test]
async fn queue_overflow_serves_stale_row_and_refreshes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |o| o.latency_ms = 200));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]).with_admission(
        Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(1),
            max_concurrent_per_engine: 3,
        }),
    );

    // Seed the row the stale request will fall back on: written two hours
    // ago with a 1 s TTL, so `get_exact` misses but `get_cache` finds it
    // expired.
    let stale_req = req("stale serve");
    let key = CacheKey::from(&stale_req);
    let seeded = SearchResponse {
        query: "stale serve".to_string(),
        results: vec![SearchResult {
            url: Url::parse("https://stale.example/").unwrap(),
            title: "stale".to_string(),
            snippet: "expired row".to_string(),
            engine: EngineId::new("replay"),
            published: None,
            score: 1.0,
        }],
        meta: SearchMeta {
            source: Source::Network,
            engines_used: vec![EngineReport {
                engine: EngineId::new("replay"),
                status: EngineStatus::Ok,
                latency_ms: 1,
                result_count: 1,
            }],
            engines_skipped: Vec::new(),
            deadline_hit: false,
            hedged: false,
            hedge_at_ms: None,
            elapsed_ms: 1,
            request_id: Uuid::now_v7(),
        },
    };
    store.entries.lock().unwrap().insert(
        key.as_str().to_string(),
        (
            seeded,
            Utc::now() - chrono::Duration::hours(2),
            Duration::from_secs(1),
        ),
    );

    // Saturate the three engine permits with distinct slow flights.
    let mut holders = Vec::new();
    for i in 0..3 {
        let pipe = pipe.clone();
        holders.push(tokio::spawn(async move {
            pipe.search(&req(&format!("holder-{i}"))).await
        }));
    }
    assert!(
        wait_until(Duration::from_secs(2), || engine.call_count() == 3).await,
        "holders never acquired their engine permits"
    );

    // The overflow: stale row served instead of 429.
    let resp = pipe.search(&stale_req).await.expect("stale serve");
    match resp.meta.source {
        Source::Cache {
            tier, stale, ttl_s, ..
        } => {
            assert_eq!(tier, Tier::T1);
            assert!(stale, "expired row must go out stale");
            assert_eq!(ttl_s, 0);
        }
        other => panic!("expected Source::Cache{{stale: true}}, got {other:?}"),
    }
    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url.as_str(), "https://stale.example/");

    // The enqueued background refresh re-runs the engine once the holders
    // release their permits, then rewrites the row fresh.
    assert!(
        wait_until(Duration::from_secs(3), || engine.call_count() == 4).await,
        "background refresh never re-ran the engine"
    );
    assert!(
        wait_until(Duration::from_secs(2), || {
            store
                .puts
                .lock()
                .unwrap()
                .iter()
                .any(|(k, _)| k == key.as_str())
        })
        .await,
        "refresh never persisted the refreshed row"
    );
    let (_, created, ttl) = store
        .entries
        .lock()
        .unwrap()
        .get(key.as_str())
        .unwrap()
        .clone();
    assert!(
        created + chrono::Duration::from_std(ttl).unwrap() > Utc::now(),
        "refreshed row must be fresh"
    );

    for h in holders {
        h.await.unwrap().expect("holder response");
    }
}

/// Overflow with a *fresh* row available serves it as a normal
/// (non-stale) tier-1 cache hit. `fail_get` fails the pre-admission
/// `get_exact` (degraded to a miss) while the overflow path's `get_cache`
/// still finds the row — the same outcome as a row that landed mid-wait.
/// Lexical is off: the seeded row shares the request's query text and
/// would otherwise be claimed by the tier-2 gate before the overflow ran.
#[tokio::test]
async fn queue_overflow_serves_fresh_row_not_stale() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |o| o.latency_ms = 200));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()])
        .with_lexical(cauce_core::LexicalConfig {
            enabled: false,
            ..Default::default()
        })
        .with_admission(Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(1),
            max_concurrent_per_engine: 1,
        }));

    // A fresh row for the overflow query (written now, 1 h TTL).
    let fresh_req = req("fresh fallback");
    let key = CacheKey::from(&fresh_req);
    let seeded = SearchResponse {
        query: "fresh fallback".to_string(),
        results: vec![SearchResult {
            url: Url::parse("https://fresh.example/").unwrap(),
            title: "fresh".to_string(),
            snippet: "fresh row".to_string(),
            engine: EngineId::new("replay"),
            published: None,
            score: 1.0,
        }],
        meta: SearchMeta {
            source: Source::Network,
            engines_used: vec![EngineReport {
                engine: EngineId::new("replay"),
                status: EngineStatus::Ok,
                latency_ms: 1,
                result_count: 1,
            }],
            engines_skipped: Vec::new(),
            deadline_hit: false,
            hedged: false,
            hedge_at_ms: None,
            elapsed_ms: 1,
            request_id: Uuid::now_v7(),
        },
    };
    store.entries.lock().unwrap().insert(
        key.as_str().to_string(),
        (seeded, Utc::now(), Duration::from_secs(3600)),
    );
    // The pre-admission lookup fails; the row is only visible to the
    // overflow path's `get_cache`.
    store.fail_get.store(true, Ordering::SeqCst);

    // Occupy the only permit.
    let holder = tokio::spawn({
        let pipe = pipe.clone();
        async move { pipe.search(&req("holder")).await }
    });
    assert!(
        wait_until(Duration::from_secs(2), || engine.call_count() == 1).await,
        "holder never started"
    );

    let resp = pipe.search(&fresh_req).await.expect("fresh row serves");
    assert!(
        matches!(
            resp.meta.source,
            Source::Cache {
                tier: Tier::T1,
                stale: false,
                ttl_s,
                ..
            } if ttl_s > 0
        ),
        "expected a fresh cache serve, got {:?}",
        resp.meta.source
    );
    assert_eq!(resp.results[0].url.as_str(), "https://fresh.example/");
    holder.await.unwrap().unwrap();
    // A fresh serve must not enqueue a refresh.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(engine.call_count(), 1);
}
