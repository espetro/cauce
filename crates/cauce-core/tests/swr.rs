//! Acceptance tests for W3-02 stale-while-revalidate (issue #45): an
//! expired tier-1 row inside `cache.stale_grace_s` is served immediately
//! `stale: true` while a deduped background refresh re-fetches the key;
//! a row past the grace window is not served at all.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use cauce_core::{
    CacheKey, CachePolicy, EngineId, EngineReport, EngineStatus, SearchMeta, SearchPipeline,
    SearchResponse, SearchResult, Source, Tier,
};
use chrono::Utc;
use support::{StubStore, replay_at, req};
use url::Url;
use uuid::Uuid;

/// Poll `cond` until it holds or `timeout` elapses (background-refresh
/// work is only observable once the spawned task has been scheduled).
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

/// Seed `store` with an already-expired row for `q`'s key: written
/// `created_ago` back with `ttl`, so `expires_at = created + ttl` sits in
/// the past. `get_exact` misses it; `get_cache` finds it stale.
fn seed_expired(
    store: &StubStore,
    q: &str,
    created_ago: chrono::Duration,
    ttl: Duration,
) -> CacheKey {
    let key = CacheKey::from(&req(q));
    let resp = SearchResponse {
        query: q.to_string(),
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
        (resp, Utc::now() - created_ago, ttl),
    );
    key
}

/// Acceptance: a row expired inside `cache.stale_grace_s` is served
/// `Source::Cache{stale: true}` in < 5 ms — the serve runs no engine and
/// never joins the admission queue — while the deduped background
/// refresh rewrites the row fresh.
#[tokio::test]
async fn in_grace_stale_row_serves_and_refreshes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |_| {}));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);

    // Written two hours ago with a 1 s TTL: ~2 h past expiry, inside the
    // default 6 h stale grace.
    let key = seed_expired(
        &store,
        "swr serve",
        chrono::Duration::hours(2),
        Duration::from_secs(1),
    );
    let stale_req = req("swr serve");

    let resp = pipe.search(&stale_req).await.expect("stale serve");
    assert!(
        resp.meta.elapsed_ms < 5,
        "stale serve must be immediate, took {} ms",
        resp.meta.elapsed_ms
    );
    match resp.meta.source {
        Source::Cache {
            tier, stale, ttl_s, ..
        } => {
            assert_eq!(tier, Tier::T1);
            assert!(stale, "in-grace expired row must go out stale");
            assert_eq!(ttl_s, 0);
        }
        other => panic!("expected Source::Cache{{stale: true}}, got {other:?}"),
    }
    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url.as_str(), "https://stale.example/");

    // The spawned refresh re-runs the engine once and rewrites the row
    // fresh — singleflight inside `spawn_refresh` is what dedupes it.
    assert!(
        wait_until(Duration::from_secs(3), || engine.call_count() == 1).await,
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

    // A second search — the acceptance's "200 ms later" — hits the
    // refreshed row: `Network`-derived contents served as a live cache hit.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let resp = pipe.search(&stale_req).await.expect("post-refresh search");
    match resp.meta.source {
        Source::Cache { tier, stale, .. } => {
            assert_eq!(tier, Tier::T1);
            assert!(!stale, "refreshed row must serve as a fresh hit");
        }
        other => panic!("expected a fresh cache hit, got {other:?}"),
    }
    assert_ne!(
        resp.results[0].url.as_str(),
        "https://stale.example/",
        "fresh hit must carry the refreshed (replay) results"
    );
    assert_eq!(
        engine.call_count(),
        1,
        "the second search came from cache; only the refresh ran an engine"
    );
}

/// A row expired *past* the grace window is not served: the pipeline
/// fans out like any other miss.
#[tokio::test]
async fn past_grace_stale_row_is_not_served() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |_| {}));
    let store = Arc::new(StubStore::default());
    let pipe =
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_cache_policy(CachePolicy {
            stale_grace: Duration::from_secs(60),
            ..CachePolicy::default()
        });

    // Expired ~2 h — far outside the 60 s grace.
    seed_expired(
        &store,
        "past grace",
        chrono::Duration::hours(2),
        Duration::from_secs(1),
    );

    let resp = pipe.search(&req("past grace")).await.expect("search");
    assert!(
        matches!(resp.meta.source, Source::Network),
        "a row past the grace window must not serve stale, got {:?}",
        resp.meta.source
    );
    assert_eq!(engine.call_count(), 1);
}

/// `stale_grace: 0` disables the stale serve entirely.
#[tokio::test]
async fn zero_grace_disables_stale_serving() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |_| {}));
    let store = Arc::new(StubStore::default());
    let pipe =
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_cache_policy(CachePolicy {
            stale_grace: Duration::ZERO,
            ..CachePolicy::default()
        });

    seed_expired(
        &store,
        "no grace",
        chrono::Duration::seconds(5),
        Duration::from_secs(1),
    );

    let resp = pipe.search(&req("no grace")).await.expect("search");
    assert!(
        matches!(resp.meta.source, Source::Network),
        "grace 0 must disable the stale serve, got {:?}",
        resp.meta.source
    );
}

/// W3-02 rule a: an all-NoResults fan-out is never `put` — a wedged
/// exec engine answering `[]` must not poison the cache.
#[tokio::test]
async fn empty_response_is_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |o| o.empty = true);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

    let resp = pipe.search(&req("empty")).await.expect("search");
    assert!(resp.results.is_empty());
    assert!(
        store.puts.lock().unwrap().is_empty(),
        "an empty response must not be cached"
    );
}

/// W3-02 rule b: a partial fan-out (any engine `Failed`) is persisted
/// with `cache.degraded_ttl_s`, not the full configured TTL.
#[tokio::test]
async fn degraded_fanout_earns_the_degraded_ttl() {
    let dir = tempfile::tempdir().unwrap();
    let ok = replay_at(dir.path(), |_| {});
    let bad = replay_at(dir.path(), |o| {
        o.id = EngineId::from("bad");
        o.blocked = true;
    });
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(ok), Arc::new(bad)])
        .with_cache_policy(CachePolicy {
            degraded_ttl: Duration::from_secs(7),
            ..CachePolicy::default()
        });

    let resp = pipe.search(&req("degraded")).await.expect("search");
    assert!(matches!(resp.meta.source, Source::Network));
    assert!(
        resp.meta
            .engines_used
            .iter()
            .any(|r| matches!(r.status, EngineStatus::Failed(_))),
        "the failed engine must report Failed: {:?}",
        resp.meta.engines_used
    );

    let puts = store.puts.lock().unwrap();
    let (_, ttl) = puts.last().expect("response must still be cached");
    assert_eq!(
        *ttl,
        Duration::from_secs(7),
        "degraded fan-out must earn the degraded TTL"
    );
}

/// A *fresh* row in the store is served by the normal `get_exact` hit —
/// the stale path never sees it.
#[tokio::test]
async fn fresh_row_still_serves_via_get_exact() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |_| {}));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);

    pipe.search(&req("fresh")).await.unwrap();
    let resp = pipe.search(&req("fresh")).await.unwrap();
    match resp.meta.source {
        Source::Cache { tier, stale, .. } => {
            assert_eq!(tier, Tier::T1);
            assert!(!stale);
        }
        other => panic!("expected a fresh cache hit, got {other:?}"),
    }
    assert_eq!(engine.call_count(), 1);
}
