//! Acceptance tests for the W3-01 P90 hedge to tier 2: a slow (or
//! short-answering) tier-1 triggers the deferred tier-2 wave at
//! `clamp(P90(tier-1 history), floor, ceiling)` inside the TTFR budget;
//! a fast tier-1 with enough results never calls tier-2 at all.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use cauce_core::{
    Admission, AdmissionLimits, EngineId, SearchOpts, SearchPipeline, Source, StreamEvent, Tier,
};
use support::{DialEngine, GateEngine, StubStore, replay_at, req};

/// Exit criterion 1: tier-1 `latency_ms = 2000`, tier-2 `latency_ms =
/// 200` → the tier-2 batch streams in well under 600 ms and the
/// terminal `meta` reports `hedged` at the floor hedge point (no latency
/// history yet, so `P90 = 0` clamps to the 300 ms floor).
#[tokio::test]
async fn slow_tier1_hedges_to_tier2_inside_ttfr_budget() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| o.latency_ms = 2_000);
    let t2 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("hedge-t2");
        o.tier = Tier::T2;
        o.latency_ms = 200;
    });
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), Arc::new(t2)]);
    let request = req("hedge acceptance");

    let started = Instant::now();
    let mut events = pipe
        .search_stream(&request, SearchOpts::default())
        .await
        .expect("pin is valid");
    let first = events.recv().await.expect("tier-2 batch must arrive first");
    let StreamEvent::Results {
        engine, elapsed_ms, ..
    } = first
    else {
        panic!("expected a results event, got {first:?}")
    };
    assert_eq!(engine, EngineId::from("hedge-t2"));
    assert!(
        elapsed_ms < 600 && started.elapsed() < Duration::from_millis(600),
        "TTFR {elapsed_ms} ms must beat the 600 ms exit bound"
    );

    // The slow tier-1 batch still lands before the terminal meta.
    let mut saw_t1 = false;
    let mut meta = None;
    while let Some(event) = events.recv().await {
        match event {
            StreamEvent::Results { engine, .. } if engine == EngineId::from("replay") => {
                saw_t1 = true;
            }
            StreamEvent::Meta(m) => {
                meta = Some(m);
                break;
            }
            _ => {}
        }
    }
    assert!(saw_t1, "the tier-1 batch still merged");
    let meta = meta.expect("terminal meta event");
    assert!(meta.meta.hedged);
    let hedge_at = meta.meta.hedge_at_ms.expect("hedge_at_ms recorded");
    assert!(
        (290..400).contains(&hedge_at),
        "empty history clamps the hedge point to the 300 ms floor, got {hedge_at}"
    );
    assert!(
        meta.meta.elapsed_ms >= 1_800,
        "the response still waits for the tier-1 straggler"
    );

    // `hedged`/`hedge_at_ms` are per-request: the cached row keeps its
    // stored network meta payload fields but the rebuilt cache-hit meta
    // resets them (same rule as `engines_skipped`).
    let hit = pipe.search(&request).await.expect("cached search");
    assert!(matches!(hit.meta.source, Source::Cache { .. }));
    assert!(!hit.meta.hedged);
    assert_eq!(hit.meta.hedge_at_ms, None);

    // The reason label for an in-flight tier-1 is `slow`.
    assert!(
        cauce_core::metrics::render_prometheus().contains("cauce_hedge_total{reason=\"slow\"}"),
        "slow-fire metric must be labelled"
    );
}

/// The collect path reports the same hedge fields.
#[tokio::test]
async fn collect_path_reports_hedged_meta() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| o.latency_ms = 2_000);
    let t2 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("hedge-t2");
        o.tier = Tier::T2;
        o.latency_ms = 200;
    });
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), Arc::new(t2)]);

    let resp = pipe.search(&req("collect hedge")).await.expect("search");
    assert!(resp.meta.hedged);
    assert!(resp.meta.hedge_at_ms.is_some());
    assert!(
        resp.meta
            .engines_used
            .iter()
            .any(|r| r.engine == EngineId::from("hedge-t2")),
        "tier-2 must appear in engines_used"
    );
    // RRF merged the tier-2 synthetic batch with tier-1's (same seeded
    // RNG → same URLs → deduped to one 10-result page).
    assert_eq!(resp.results.len(), 10);
}

/// Acceptance: with tier-1 fast (`latency_ms = 100`, 10 results) tier 2
/// is never called — the primary page clears `min_results` before the
/// hedge point.
#[tokio::test]
async fn fast_tier1_never_calls_tier2() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| o.latency_ms = 100);
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("hedge-t2");
        o.tier = Tier::T2;
        o.latency_ms = 200;
    }));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()]);

    let resp = pipe.search(&req("fast primary")).await.expect("search");
    assert!(!resp.meta.hedged);
    assert_eq!(resp.meta.hedge_at_ms, None);
    assert_eq!(t2.call_count(), 0, "tier-2 must stay deferred");
    assert!(
        resp.meta.engines_skipped.is_empty(),
        "an unfired hedge reports no skips"
    );
}

/// `reason = "few"`: every tier-1 answered but the merged page stayed
/// under `min_results`, so the hedge fires early rather than waiting for
/// the hedge point — `hedge_at_ms` lands well under the 300 ms floor.
#[tokio::test]
async fn short_primary_page_fires_hedge_early() {
    let dir = tempfile::tempdir().unwrap();
    // `empty` answers NoResults inside a few ms.
    let t1 = replay_at(dir.path(), |o| o.empty = true);
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("hedge-t2");
        o.tier = Tier::T2;
        o.latency_ms = 200;
    }));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()]);

    let resp = pipe.search(&req("few results")).await.expect("search");
    assert!(resp.meta.hedged);
    let hedge_at = resp.meta.hedge_at_ms.expect("hedge fired");
    assert!(
        hedge_at < 300,
        "an all-answered short page fires before the floor, got {hedge_at}"
    );
    assert_eq!(t2.call_count(), 1);
    assert_eq!(resp.results.len(), 10);
    assert!(
        cauce_core::metrics::render_prometheus().contains("cauce_hedge_total{reason=\"few\"}"),
        "early-fire metric must be labelled few"
    );
}

/// A deferred engine must not reserve its permit upfront: with
/// `max_concurrent_per_engine = 1` a long tier-2 call occupying the
/// semaphore cannot `RateLimited` a flight whose primaries need no
/// hedge — tier-2 capacity is only acquired inside the fetch when a
/// hedge or promotion actually needs it.
#[tokio::test]
async fn deferred_tier2_does_not_consume_upfront_permits() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("t1");
        o.latency_ms = 100;
    });
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("t2");
        o.tier = Tier::T2;
        o.latency_ms = 1_200;
    }));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()]).with_admission(
        Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(50),
            max_concurrent_per_engine: 1,
        }),
    );

    // Occupy the single tier-2 permit with a pinned tier-2-only search
    // (the promoted wave acquires inside its fetch and holds for the
    // whole 1200 ms call).
    let mut pinned = req("occupy tier two");
    pinned.engines = Some(vec![EngineId::from("t2")]);
    let pipe_occ = pipe.clone();
    let occupier = tokio::spawn(async move { pipe_occ.search(&pinned).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The fast tier-1 page fills min_results before the hedge point, so
    // this flight never touches tier-2 capacity.
    let resp = pipe
        .search(&req("healthy primaries"))
        .await
        .expect("primary-only flight must not queue behind tier-2");
    assert!(!resp.meta.hedged);
    assert_eq!(
        resp.meta.engines_used.len(),
        1,
        "only the tier-1 engine ran"
    );

    let occ = occupier.await.unwrap().expect("occupier search");
    assert_eq!(
        occ.meta.engines_used.len(),
        1,
        "the pinned flight ran tier-2 alone"
    );
}

/// The other half of the split: when the hedge does trigger while
/// tier-2 capacity is saturated, the flight waits for the permit inside
/// its remaining budget and fires when it frees — it is not rejected
/// at fan-out like an exhausted primary queue.
#[tokio::test]
async fn hedge_waits_for_tier2_permit_then_fires() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("t1");
        o.latency_ms = 2_000;
    });
    // The occupier's 1 s tier-2 call holds the only permit past the
    // 300 ms hedge point, so the hedged flight must wait for it.
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("t2");
        o.tier = Tier::T2;
        o.latency_ms = 1_000;
    }));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()]).with_admission(
        Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(50),
            max_concurrent_per_engine: 1,
        }),
    );

    let mut pinned = req("occupy tier two");
    pinned.engines = Some(vec![EngineId::from("t2")]);
    let pipe_occ = pipe.clone();
    let occupier = tokio::spawn(async move { pipe_occ.search(&pinned).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Tier-1 is slow: the hedge triggers at the 300 ms floor, waits for
    // the occupier to free the tier-2 permit (~1 s), then fires.
    let resp = pipe
        .search(&req("hedge behind saturated tier2"))
        .await
        .expect("hedge must wait for the permit, not fail");
    assert!(resp.meta.hedged);
    let hedge_at = resp.meta.hedge_at_ms.expect("hedge fired");
    assert!(
        hedge_at >= 900,
        "the hedge fires only once the tier-2 permit frees, got {hedge_at}"
    );
    assert!(t2.call_count() >= 2);
    occupier.await.unwrap().expect("occupier search");
}

/// The hedge clock starts at fan-out, not request start: a slow
/// lexical lookup (pre-fan-out work) must not pre-consume the floor the
/// primaries were promised. Here the 500 ms lookup exceeds the 300 ms
/// floor, yet the fast tier-1 still clears `min_results` first and the
/// hedge is cancelled rather than fired at t=0.
#[tokio::test]
async fn pre_fanout_work_does_not_eat_the_hedge_floor() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("t1");
        o.latency_ms = 100;
    });
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("t2");
        o.tier = Tier::T2;
        o.latency_ms = 200;
    }));
    let store = Arc::new(StubStore::default());
    store
        .lexical_delay_ms
        .store(500, std::sync::atomic::Ordering::SeqCst);
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()]);

    let resp = pipe.search(&req("slow lexical")).await.expect("search");
    assert!(
        !resp.meta.hedged,
        "pre-fan-out delay must not fire the hedge"
    );
    assert_eq!(t2.call_count(), 0);
}

/// A hedge point past the hard deadline is cancelled outright: the
/// loop must not sleep to the floor, gate the deferred wave, and spawn
/// zero-budget calls after the deadline already elapsed.
#[tokio::test]
async fn hard_deadline_cancels_a_late_hedge() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = replay_at(dir.path(), |o| {
        o.id = EngineId::from("t1");
        o.latency_ms = 400;
    });
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("t2");
        o.tier = Tier::T2;
        o.latency_ms = 50;
    }));
    let store = Arc::new(StubStore::default());
    // Deadline 100 ms < hedge floor 300 ms: the hedge can never fire.
    let pipe = SearchPipeline::new(store, vec![Arc::new(t1), t2.clone()])
        .with_deadline(Duration::from_millis(100));

    let started = Instant::now();
    let outcome = pipe.search(&req("tight deadline")).await;
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "the response must return near the deadline, not at the hedge point ({:?})",
        started.elapsed()
    );
    assert!(outcome.is_err(), "the timed-out primary reports an error");
    assert_eq!(
        t2.call_count(),
        0,
        "a post-deadline hedge must not spawn or gate"
    );
}

/// The hedge point pools P90 over the engines this request actually
/// admitted: a breaker-skipped tier-1's slow history must not postpone
/// the hedge for the healthy set. t1a builds a 1500 ms history then
/// opens its breaker; t1b's own history is 100 ms. If the pool included
/// t1a the hedge point would clamp to the 1500 ms ceiling and the
/// all-answered short page would fire `reason=few` at ~800 ms instead —
/// so the assertion is on `hedge_at_ms` near the floor.
#[tokio::test]
async fn skipped_engine_history_does_not_delay_hedge() {
    let dir = tempfile::tempdir().unwrap();
    let t1a = Arc::new(GateEngine::new(
        replay_at(dir.path(), |o| {
            o.id = EngineId::from("t1a");
            o.latency_ms = 1_500;
        }),
        true,
    ));
    let t1b = Arc::new(DialEngine::new(
        replay_at(dir.path(), |o| {
            o.id = EngineId::from("t1b");
            o.empty = true;
        }),
        100,
    ));
    let t2 = Arc::new(replay_at(dir.path(), |o| {
        o.id = EngineId::from("t2");
        o.tier = Tier::T2;
        o.latency_ms = 100;
    }));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![t1a.clone(), t1b.clone(), t2.clone()]);

    // History: one call gives t1a a ~1500 ms sample and t1b a ~100 ms
    // one (distinct queries so the cache never short-circuits).
    pipe.search(&req("warmup one")).await.expect("warmup");
    // A single Blocked call opens t1a's breaker for 15 min.
    t1a.set_healthy(false);
    pipe.search(&req("warmup two")).await.expect("warmup");

    // t1b now answers short but slowly: the hedge point over the gated
    // set {t1b} is the 300 ms floor (P90 100), while pooling t1a's
    // history would push it to the ceiling.
    t1b.set_latency_ms(800);
    let resp = pipe.search(&req("measured")).await.expect("search");
    assert!(resp.meta.hedged, "the short page still hedges");
    let hedge_at = resp.meta.hedge_at_ms.expect("hedge fired");
    assert!(
        hedge_at < 600,
        "gated-only P90 fires near the floor; pooling the skipped \
         engine's history would land ~800 ms, got {hedge_at}"
    );
}
