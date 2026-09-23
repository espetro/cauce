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

use cauce_core::{EngineId, SearchOpts, SearchPipeline, Source, StreamEvent, Tier};
use support::{StubStore, replay_at, req};

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
