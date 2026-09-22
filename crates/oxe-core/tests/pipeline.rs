//! Acceptance tests for `SearchPipeline` (W0-08): deadline fan-out,
//! unconditional `search_log`, RRF merge, failure and TTL semantics, all
//! driven on `Replay` engines and a recording stub `Store`.
//!
//! Integration test rather than a unit module deliberately: `oxe-engines`
//! is a cyclic dev-dependency, and only an integration test links the same
//! `oxe-core` build `Replay` implements `Engine` against.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use oxe_core::{
    CacheKey, EngineError, EngineId, EngineStatus, LogSource, PipelineError, SearchOpts,
    SearchPipeline, SearchResult, Source, Tier, normalize_url,
};
use oxe_engines::{Cassette, cassette_path};
use support::{StubStore, replay_at, req};
use url::Url;
use uuid::Uuid;

/// Acceptance: two replay engines, one `latency_ms = 5000`, deadline
/// 3000 ms → response at ~3000 ms, `deadline_hit`, fast results in.
#[tokio::test]
async fn deadline_drops_straggler_and_serves_fast_engine() {
    let dir = tempfile::tempdir().unwrap();
    let fast = replay_at(dir.path(), |_| {});
    let slow = replay_at(dir.path(), |o| o.latency_ms = 5_000);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(fast), Arc::new(slow)]);

    let t0 = Instant::now();
    let resp = pipe.search(&req("tail latency")).await.unwrap();
    let elapsed = t0.elapsed();

    assert!(
        elapsed >= Duration::from_millis(2_900) && elapsed < Duration::from_millis(4_500),
        "expected ~3 s at the deadline, got {elapsed:?}"
    );
    assert!(resp.meta.deadline_hit);
    assert_eq!(resp.results.len(), 10, "fast engine's synthetic page");
    assert!(matches!(resp.meta.source, Source::Network));

    assert_eq!(resp.meta.engines_used.len(), 2);
    assert!(
        resp.meta
            .engines_used
            .iter()
            .any(|r| r.status == EngineStatus::Ok && r.result_count == 10)
    );
    assert!(
        resp.meta.engines_used.iter().any(|r| r.status
            == EngineStatus::Failed(EngineError::Timeout)
            && r.latency_ms >= 2_900)
    );

    let logs = store.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    let row = &logs[0];
    assert_eq!(row.source, LogSource::Network);
    assert_eq!(row.tier, None);
    assert_eq!(row.query_hash, CacheKey::from(&req("tail latency")));
    assert_eq!(row.query, "tail latency");
    assert_eq!(row.result_count, 10);
    assert_eq!(row.engines.len(), 2);
    assert!(row.deadline_hit);
    assert!(row.latency_ms >= 2_900);
}

/// Acceptance: a cache hit is served in < 5 ms and still appends a
/// `search_log` row with `source = cache`.
#[tokio::test]
async fn cache_hit_is_fast_and_still_logged() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(replay_at(dir.path(), |_| {}));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);
    let request = req("Cached   QUERY");

    let first = pipe.search(&request).await.unwrap();
    assert!(matches!(first.meta.source, Source::Network));
    assert_eq!(engine.call_count(), 1);
    assert_eq!(first.query, "cached query", "resp.query is normalised");

    let t0 = Instant::now();
    let second = pipe.search(&request).await.unwrap();
    let elapsed = t0.elapsed();

    assert!(
        elapsed < Duration::from_millis(5),
        "cache hit took {elapsed:?}"
    );
    assert_eq!(engine.call_count(), 1, "engine must not run on a hit");
    match second.meta.source {
        Source::Cache {
            tier,
            age_s,
            ttl_s,
            stale,
        } => {
            assert_eq!(tier, Tier::T1);
            assert!(!stale);
            assert_eq!(age_s, 0);
            assert!((3_500..=3_600).contains(&ttl_s), "remaining ttl {ttl_s}");
        }
        Source::Network => panic!("expected cache hit"),
    }
    assert_eq!(second.results, first.results);
    assert_ne!(second.meta.request_id, first.meta.request_id);

    let logs = store.logs.lock().unwrap();
    assert_eq!(logs.len(), 2, "hit path must also log");
    assert_eq!(logs[0].source, LogSource::Network);
    assert_eq!(logs[1].source, LogSource::Cache);
    assert_eq!(logs[1].tier, Some(Tier::T1));
    assert_eq!(logs[1].result_count, 10);
    assert_eq!(logs[1].engines, vec![EngineId::from("replay")]);
    assert_eq!(logs[1].query_hash, CacheKey::from(&request));
}

/// Acceptance: RRF sums contributions, so a URL returned by both engines
/// outranks every single-engine URL.
#[tokio::test]
async fn rrf_url_returned_by_both_engines_ranks_first() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let q = "rrf merge";
    let res = |url: &str, engine: &str| SearchResult {
        url: Url::parse(url).unwrap(),
        title: url.to_string(),
        snippet: "snippet".to_string(),
        engine: EngineId::from(engine),
        published: None,
        score: 0.0,
    };
    // eng_a: shared at rank 3; eng_b: shared at rank 2.
    Cassette::new(
        EngineId::from("eng_a"),
        q,
        vec![
            res("https://a1.example.com/", "eng_a"),
            res("https://a2.example.com/", "eng_a"),
            res("https://example.com/shared?utm_source=nl", "eng_a"),
        ],
    )
    .save(&cassette_path(root, "eng_a", q))
    .unwrap();
    Cassette::new(
        EngineId::from("eng_b"),
        q,
        vec![
            res("https://b1.example.com/", "eng_b"),
            res("https://example.com/shared", "eng_b"),
            res("https://b2.example.com/", "eng_b"),
        ],
    )
    .save(&cassette_path(root, "eng_b", q))
    .unwrap();

    let a = replay_at(root, |o| o.cassette_engine = Some(EngineId::from("eng_a")));
    let b = replay_at(root, |o| o.cassette_engine = Some(EngineId::from("eng_b")));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(a), Arc::new(b)]);

    let resp = pipe.search(&req(q)).await.unwrap();
    assert_eq!(resp.results.len(), 5, "shared URL deduped");
    assert_eq!(
        normalize_url(&resp.results[0].url),
        Url::parse("https://example.com/shared").unwrap()
    );
    // 1/(60+3) + 1/(60+2): both engines' contributions summed.
    let expected = 1.0f32 / 63.0 + 1.0f32 / 62.0;
    assert!((resp.results[0].score - expected).abs() < 1e-6);
    // The best single contribution (eng_b, rank 2) supplies the row.
    assert_eq!(resp.results[0].engine, EngineId::from("eng_b"));
    // Single-engine ties keep first-seen order (eng_a listed first).
    assert_eq!(resp.results[1].url.as_str(), "https://a1.example.com/");
    assert_eq!(resp.results[2].url.as_str(), "https://b1.example.com/");
}

/// All engines fail → `AllEnginesFailed` carrying both errors; the
/// `search_log` row is still written and nothing is cached.
#[tokio::test]
async fn all_engines_failed_returns_errors_logs_and_skips_cache() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = replay_at(dir.path(), |o| o.blocked = true);
    let flaky = replay_at(dir.path(), |o| o.fail_every = 1);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(blocked), Arc::new(flaky)]);

    let err = pipe.search(&req("doomed")).await.unwrap_err();
    match err {
        PipelineError::AllEnginesFailed(failures) => {
            assert_eq!(failures.len(), 2);
            assert!(
                failures
                    .iter()
                    .any(|(_, e)| matches!(e, EngineError::Blocked))
            );
            assert!(
                failures
                    .iter()
                    .any(|(_, e)| matches!(e, EngineError::Transport(_)))
            );
        }
        other => panic!("expected AllEnginesFailed, got {other:?}"),
    }

    assert!(store.puts.lock().unwrap().is_empty(), "never cached");
    let logs = store.logs.lock().unwrap();
    assert_eq!(logs.len(), 1, "failure path still logs");
    assert_eq!(logs[0].source, LogSource::Network);
    assert_eq!(logs[0].result_count, 0);
    assert_eq!(logs[0].engines.len(), 2);
}

/// Zero results with a healthy engine is a valid, cacheable response.
#[tokio::test]
async fn empty_results_are_valid_and_cached() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |o| o.empty = true);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

    let resp = pipe.search(&req("nothing here")).await.unwrap();
    assert!(resp.results.is_empty());
    assert!(!resp.meta.deadline_hit);
    assert_eq!(store.puts.lock().unwrap().len(), 1, "cached normally");
    assert_eq!(store.logs.lock().unwrap()[0].result_count, 0);
}

/// `search_with_id` echoes the caller's id; `search_opts.ttl` is clamped
/// to `ttl_cap`; the pin filters the fan-out.
#[tokio::test]
async fn request_id_ttl_override_and_engine_pin() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |_| {});
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

    let id = Uuid::now_v7();
    let resp = pipe.search_with_id(&req("ids"), id).await.unwrap();
    assert_eq!(resp.meta.request_id, id);
    assert_eq!(
        store.puts.lock().unwrap()[0].1,
        Duration::from_secs(3_600),
        "default ttl"
    );

    let over = pipe
        .search_opts(
            &req("huge ttl"),
            SearchOpts {
                request_id: None,
                ttl: Some(Duration::from_secs(999_999)),
            },
        )
        .await
        .unwrap();
    assert!(matches!(over.meta.source, Source::Network));
    assert_eq!(
        store.puts.lock().unwrap()[1].1,
        Duration::from_secs(86_400),
        "ttl clamped to cap"
    );

    let under = pipe
        .search_opts(
            &req("small ttl"),
            SearchOpts {
                request_id: None,
                ttl: Some(Duration::from_secs(60)),
            },
        )
        .await
        .unwrap();
    assert!(matches!(under.meta.source, Source::Network));
    assert_eq!(store.puts.lock().unwrap()[2].1, Duration::from_secs(60));

    // Pin to a configured engine runs it; pin to an unknown id is
    // `NoEngines` (still logged).
    let mut pinned = req("pinned");
    pinned.engines = Some(vec![EngineId::from("replay")]);
    pipe.search(&pinned).await.unwrap();
    pinned.engines = Some(vec![EngineId::from("nope")]);
    let err = pipe.search(&pinned).await.unwrap_err();
    assert!(matches!(err, PipelineError::NoEngines));
    let last = store.logs.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.source, LogSource::Network);
    assert!(last.engines.is_empty());
}

/// `NoResults` is an answer, not a failure: an all-`NoResults` fan-out
/// (replay `page_limit` exceeded) is a 200-shaped empty response that is
/// persisted and logged like any network result, not `AllEnginesFailed`.
#[tokio::test]
async fn all_no_results_is_empty_ok_response() {
    let dir = tempfile::tempdir().unwrap();
    let a = replay_at(dir.path(), |o| o.page_limit = Some(0));
    let b = replay_at(dir.path(), |o| o.page_limit = Some(0));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(a), Arc::new(b)]);

    let resp = pipe.search(&req("past the last page")).await.unwrap();
    assert!(resp.results.is_empty());
    assert!(matches!(resp.meta.source, Source::Network));
    assert!(!resp.meta.deadline_hit);
    assert_eq!(resp.meta.engines_used.len(), 2);
    assert!(
        resp.meta
            .engines_used
            .iter()
            .all(|r| r.status == EngineStatus::Failed(EngineError::NoResults))
    );

    assert_eq!(store.puts.lock().unwrap().len(), 1, "empty 200 is cached");
    let logs = store.logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].source, LogSource::Network);
    assert_eq!(logs[0].result_count, 0);
    assert_eq!(logs[0].engines.len(), 2);
}

/// `page` beyond a replay's `page_limit` → `NoResults` → empty 200-path.
#[tokio::test]
async fn page_beyond_limit_is_empty_ok_response() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |o| o.page_limit = Some(1));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(engine)]);

    let mut request = req("paging");
    assert_eq!(pipe.search(&request).await.unwrap().results.len(), 10);
    request.page = 2;
    let resp = pipe.search(&request).await.unwrap();
    assert!(resp.results.is_empty());
    assert!(matches!(resp.meta.source, Source::Network));
    assert_eq!(
        resp.meta.engines_used[0].status,
        EngineStatus::Failed(EngineError::NoResults)
    );
}

/// Documented semantic: at least one engine answered (`Ok` or
/// `NoResults`) → success response. `NoResults` + a deadline timeout is
/// an empty 200 with `deadline_hit`, not `AllEnginesFailed`.
#[tokio::test]
async fn no_results_plus_timeout_is_empty_ok_response() {
    let dir = tempfile::tempdir().unwrap();
    let no_results = replay_at(dir.path(), |o| o.page_limit = Some(0));
    let slow = replay_at(dir.path(), |o| o.latency_ms = 2_000);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(no_results), Arc::new(slow)])
        .with_deadline(Duration::from_millis(300));

    let resp = pipe.search(&req("mixed answers")).await.unwrap();
    assert!(resp.results.is_empty());
    assert!(resp.meta.deadline_hit);
    assert_eq!(
        resp.meta.engines_used[0].status,
        EngineStatus::Failed(EngineError::NoResults)
    );
    assert_eq!(
        resp.meta.engines_used[1].status,
        EngineStatus::Failed(EngineError::Timeout)
    );
    assert_eq!(store.logs.lock().unwrap().len(), 1);
}

/// A `get_exact` failure degrades to a miss instead of an error.
#[tokio::test]
async fn cache_lookup_failure_is_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |_| {});
    let store = Arc::new(StubStore::default());
    store.fail_get.store(true, Ordering::SeqCst);
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

    let resp = pipe.search(&req("degraded")).await.unwrap();
    assert!(matches!(resp.meta.source, Source::Network));
    assert_eq!(resp.results.len(), 10);
}
