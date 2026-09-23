//! Acceptance tests for `cauce-core::health` (W1-06): EWMA latency
//! (`alpha = 0.3`), consecutive-failure counting, the Closed/Open/HalfOpen
//! breaker, debounced `put_health` persistence, and `load` restoring
//! persisted state — all driven through `SearchPipeline` on `GateEngine`
//! (a `Replay` behind a health gate) and the recording `StubStore`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::sync::Arc;
use std::time::Duration;

use cauce_core::{
    BreakerState, EngineError, EngineId, Gate, HealthPolicy, HealthTracker, PipelineError,
    SearchPipeline, Store,
};
use cauce_engines::cassette_key;
use support::{GateEngine, StubStore, replay_at, req};
use uuid::Uuid;

fn breaker_open(err: &PipelineError) -> &Vec<EngineId> {
    match err {
        PipelineError::BreakerOpen(ids) => ids,
        other => panic!("expected BreakerOpen, got {other:?}"),
    }
}

/// Acceptance: a `blocked=true` replay opens after one call and is skipped
/// for the next; the `Open` row and the `engine.breaker` audit are
/// persisted before the first response returns.
#[tokio::test]
async fn blocked_engine_opens_after_one_call_and_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(GateEngine::new(replay_at(dir.path(), |_| {}), false));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);

    let err = pipe.search(&req("breaker one")).await.unwrap_err();
    match &err {
        PipelineError::AllEnginesFailed(failures) => {
            assert!(failures.iter().any(|(_, e)| *e == EngineError::Blocked));
        }
        other => panic!("expected AllEnginesFailed, got {other:?}"),
    }
    assert_eq!(engine.call_count(), 1);

    let row = pipe
        .health()
        .health_row(&EngineId::from("replay"))
        .expect("replay health row");
    assert_eq!(row.breaker, BreakerState::Open);
    assert_eq!(row.failures, 1);
    let until = row.breaker_until.expect("open breaker has a window");
    assert!(until > chrono::Utc::now(), "window must be in the future");
    // 15 min settled window, with slack for slow CI.
    assert!(until <= chrono::Utc::now() + chrono::Duration::minutes(15));
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|e| e.contains("blocked"))
    );

    // The transition was flushed urgently (not debounced): the persisted
    // row is already Open, and the transition was audited. Scope the
    // lock guards so they drop before the next `.await`.
    let stored_breaker = store
        .health_rows
        .lock()
        .unwrap()
        .get("replay")
        .map(|r| r.breaker);
    assert_eq!(
        stored_breaker,
        Some(BreakerState::Open),
        "health row persisted urgently"
    );
    let audited = {
        let audits = store.audits.lock().unwrap();
        audits.iter().any(|a| {
            a.action == "engine.breaker" && a.target == "replay" && a.details["to"] == "open"
        })
    };
    assert!(audited, "breaker transition must be audited");

    // The next request skips the engine entirely.
    let err = pipe.search(&req("breaker two")).await.unwrap_err();
    assert_eq!(breaker_open(&err), &vec![EngineId::from("replay")]);
    assert_eq!(
        engine.call_count(),
        1,
        "open engine must not be called again"
    );
}

/// Acceptance: after the window the engine is probed once — a failed probe
/// re-opens it, a later successful probe closes it.
#[tokio::test]
async fn half_open_probes_once_then_reopens_and_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(GateEngine::new(replay_at(dir.path(), |_| {}), false));
    let store = Arc::new(StubStore::default());
    let pipe =
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_health_policy(HealthPolicy {
            abuse_window: Duration::from_millis(80),
            ..HealthPolicy::default()
        });

    pipe.search(&req("p1")).await.unwrap_err();
    assert_eq!(engine.call_count(), 1);

    // Still inside the window: skipped.
    pipe.search(&req("p2")).await.unwrap_err();
    assert_eq!(engine.call_count(), 1, "skipped inside the open window");

    // Past the window: exactly one probe goes through; it fails and the
    // breaker re-opens.
    tokio::time::sleep(Duration::from_millis(100)).await;
    pipe.search(&req("p3")).await.unwrap_err();
    assert_eq!(engine.call_count(), 2, "one probe after the window");
    let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
    assert_eq!(row.breaker, BreakerState::Open, "failed probe re-opens");

    // Re-opened: skipped again.
    pipe.search(&req("p4")).await.unwrap_err();
    assert_eq!(engine.call_count(), 2);

    // The engine recovers upstream; the next window's probe closes the
    // breaker and the following request fans out normally.
    engine.set_healthy(true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let resp = pipe.search(&req("p5")).await.unwrap();
    assert_eq!(resp.results.len(), 10);
    assert_eq!(engine.call_count(), 3, "recovery probe ran once");
    let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
    assert_eq!(row.breaker, BreakerState::Closed, "good probe closes");
    assert_eq!(row.failures, 0);
    assert!(row.last_ok_at.is_some());

    let resp = pipe.search(&req("p6")).await.unwrap();
    assert_eq!(engine.call_count(), 4, "closed engine fans out normally");
    assert_eq!(resp.results.len(), 10);
}

/// Two concurrent requests past the window produce exactly one probe call.
#[tokio::test]
async fn half_open_probe_is_single_flight() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(GateEngine::new(replay_at(dir.path(), |_| {}), false));
    let store = Arc::new(StubStore::default());
    let pipe = Arc::new(
        SearchPipeline::new(store, vec![engine.clone()]).with_health_policy(HealthPolicy {
            abuse_window: Duration::from_millis(60),
            ..HealthPolicy::default()
        }),
    );

    pipe.search(&req("c0")).await.unwrap_err();
    assert_eq!(engine.call_count(), 1);
    tokio::time::sleep(Duration::from_millis(80)).await;

    let (r1, r2) = (req("c1"), req("c2"));
    let (a, b) = tokio::join!(pipe.search(&r1), pipe.search(&r2));
    assert!(a.is_err() && b.is_err());
    assert_eq!(
        engine.call_count(),
        2,
        "exactly one probe across concurrent requests"
    );
}

/// Settled rule: 3 consecutive `Timeout`s open the breaker for 5 min.
/// `Parse`/`Transport` count failures and feed the W3-07 degraded streak
/// — kept under its default threshold of 5 here so this test stays about
/// the timeout rule.
#[tokio::test]
async fn three_consecutive_timeouts_open_breaker() {
    let dir = tempfile::tempdir().unwrap();
    let slow = replay_at(dir.path(), |o| o.latency_ms = 5_000);
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(slow)])
        .with_deadline(Duration::from_millis(50));

    for i in 1..=2 {
        pipe.search(&req(&format!("timeout {i}")))
            .await
            .unwrap_err();
        let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
        assert_eq!(row.failures, i);
        assert_eq!(row.breaker, BreakerState::Closed, "opened too early");
        assert!(row.ewma_ms >= 50.0, "timeout latency feeds the EWMA");
    }

    pipe.search(&req("timeout 3")).await.unwrap_err();
    let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
    assert_eq!(row.failures, 3);
    assert_eq!(row.breaker, BreakerState::Open, "3 timeouts open it");
    let until = row.breaker_until.unwrap();
    assert!(until <= chrono::Utc::now() + chrono::Duration::minutes(5));

    // `Transport`/`Parse` increment failures and feed the W3-07 degraded
    // streak; under the default threshold of 5 a run of four still does
    // not open the breaker.
    let tracker = HealthTracker::new(store);
    let id = EngineId::from("flaky");
    let err = |e: EngineError| {
        tracker.record_err(&id, Duration::from_millis(10), &e, Uuid::now_v7());
    };
    for _ in 0..4 {
        err(EngineError::Transport("boom".to_string()));
    }
    let row = tracker.health_row(&id).unwrap();
    assert_eq!(row.failures, 4);
    assert_eq!(row.breaker, BreakerState::Closed);

    // "3 consecutive timeouts" is a streak of its own: a `Timeout`
    // between the degraded errors resets the degraded streak, and
    // Parse, Parse, Timeout leaves the breaker Closed even though
    // `failures` is already 8.
    err(EngineError::Timeout);
    err(EngineError::Parse("bad html".to_string()));
    err(EngineError::Parse("bad html".to_string()));
    err(EngineError::Timeout);
    let row = tracker.health_row(&id).unwrap();
    assert_eq!(row.failures, 8);
    assert_eq!(
        row.breaker,
        BreakerState::Closed,
        "a non-Timeout error between timeouts breaks the streak"
    );

    // The streak then accumulates from 1: two more timeouts open it.
    err(EngineError::Timeout);
    err(EngineError::Timeout);
    let row = tracker.health_row(&id).unwrap();
    assert_eq!(row.breaker, BreakerState::Open, "3 consecutive timeouts");

    // And a non-consecutive Timeout run resets on any answer.
    let id2 = EngineId::from("flaky2");
    let err2 = |e: EngineError| {
        tracker.record_err(&id2, Duration::from_millis(10), &e, Uuid::now_v7());
    };
    err2(EngineError::Timeout);
    err2(EngineError::Timeout);
    tracker.record_ok(&id2, Duration::from_millis(10), Uuid::now_v7());
    err2(EngineError::Timeout);
    let row = tracker.health_row(&id2).unwrap();
    assert_eq!(row.failures, 1, "a success resets consecutive failures");
    assert_eq!(row.breaker, BreakerState::Closed);
}

/// A malformed cassette at `<root>/<engine>/<sha8>.json`: the read
/// succeeds, the decode fails, so `replay` answers `EngineError::Parse`
/// on every call for `query` — the drifted-selector shape W3-07 rests.
fn write_bad_cassette(root: &std::path::Path, query: &str) {
    let dir = root.join("replay");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.json", cassette_key(query))),
        "{ not a cassette",
    )
    .unwrap();
}

/// Acceptance (W3-07): a replay engine scripted to `Parse` 5 times in a
/// row opens the breaker on the 5th and is skipped until the
/// `degraded_window` (10 min) elapses.
#[tokio::test]
async fn five_consecutive_parse_errors_open_breaker() {
    let dir = tempfile::tempdir().unwrap();
    write_bad_cassette(dir.path(), "drifted selector");
    let engine = Arc::new(GateEngine::new(replay_at(dir.path(), |_| {}), true));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![engine.clone()]);

    for i in 1..=4 {
        let err = pipe.search(&req("drifted selector")).await.unwrap_err();
        match &err {
            PipelineError::AllEnginesFailed(failures) => {
                assert!(
                    failures
                        .iter()
                        .any(|(_, e)| matches!(e, EngineError::Parse(_)))
                );
            }
            other => panic!("expected AllEnginesFailed, got {other:?}"),
        }
        let row = pipe
            .health()
            .health_row(&EngineId::from("replay"))
            .expect("replay health row");
        assert_eq!(row.failures, i);
        assert_eq!(row.breaker, BreakerState::Closed, "opened on streak {i}");
    }

    // The 5th consecutive `Parse` opens the breaker for
    // `degraded_window`; the transition persists urgently like the
    // timeout/abuse ones.
    pipe.search(&req("drifted selector")).await.unwrap_err();
    let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
    assert_eq!(row.failures, 5);
    assert_eq!(row.breaker, BreakerState::Open, "5 parses open it");
    let until = row.breaker_until.expect("open breaker has a window");
    assert!(until > chrono::Utc::now(), "window must be in the future");
    assert!(until <= chrono::Utc::now() + chrono::Duration::minutes(10));
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|e| e.contains("parse error"))
    );
    let stored_breaker = store
        .health_rows
        .lock()
        .unwrap()
        .get("replay")
        .map(|r| r.breaker);
    assert_eq!(
        stored_breaker,
        Some(BreakerState::Open),
        "health row persisted urgently"
    );

    // Inside the window the engine is skipped entirely.
    let err = pipe.search(&req("drifted selector")).await.unwrap_err();
    assert_eq!(breaker_open(&err), &vec![EngineId::from("replay")]);
    assert_eq!(
        engine.call_count(),
        5,
        "open engine must not be called again"
    );
}

/// Acceptance (W3-07): the degraded streak is *consecutive* — an `Ok` or
/// a `NoResults` answer between `Parse`/`Transport` errors resets it, so
/// a flaky engine that still answers sometimes never trips the breaker.
#[tokio::test]
async fn degraded_streak_interrupted_by_answers_does_not_open() {
    let dir = tempfile::tempdir().unwrap();
    write_bad_cassette(dir.path(), "bad");
    // `page_limit = 1` lets page 2 answer `NoResults` (recorded as an
    // answer, like `Ok`) while page 1 keeps producing `Parse`.
    let engine = Arc::new(GateEngine::new(
        replay_at(dir.path(), |o| o.page_limit = Some(1)),
        true,
    ));
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store, vec![engine.clone()]);
    let replay = EngineId::from("replay");
    let parse_err = |e: &PipelineError| match e {
        PipelineError::AllEnginesFailed(failures) => {
            assert!(
                failures
                    .iter()
                    .any(|(_, e)| matches!(e, EngineError::Parse(_)))
            );
        }
        other => panic!("expected AllEnginesFailed, got {other:?}"),
    };

    // 4 `Parse`s, an `Ok` answer, 4 more `Parse`s: the answer reset the
    // streak, so the breaker stays Closed.
    for _ in 0..4 {
        parse_err(&pipe.search(&req("bad")).await.unwrap_err());
    }
    assert_eq!(pipe.search(&req("good")).await.unwrap().results.len(), 10);
    for _ in 0..4 {
        parse_err(&pipe.search(&req("bad")).await.unwrap_err());
    }
    let row = pipe.health().health_row(&replay).unwrap();
    assert_eq!(row.failures, 4);
    assert_eq!(
        row.breaker,
        BreakerState::Closed,
        "an `Ok` between `Parse`s breaks the streak"
    );

    // `NoResults` is an answer too (the pipeline records it through
    // `record_ok`): it resets the streak as well.
    let mut page2 = req("bad");
    page2.page = 2;
    let resp = pipe.search(&page2).await.unwrap();
    assert!(resp.results.is_empty(), "page 2 is an empty answer");
    for _ in 0..4 {
        parse_err(&pipe.search(&req("bad")).await.unwrap_err());
    }
    let row = pipe.health().health_row(&replay).unwrap();
    assert_eq!(row.failures, 4);
    assert_eq!(
        row.breaker,
        BreakerState::Closed,
        "a `NoResults` between `Parse`s breaks the streak"
    );
    assert_eq!(engine.call_count(), 14);

    // `Parse` and `Transport` share one streak, and a `Timeout` between
    // them breaks it (it feeds the timeout streak instead).
    let tracker = HealthTracker::new(Arc::new(StubStore::default()));
    let id = EngineId::from("drifty");
    let rec = |e: EngineError| {
        tracker.record_err(&id, Duration::from_millis(10), &e, Uuid::now_v7());
    };
    for i in 1..=4 {
        // Alternating kinds still accumulate into the one streak.
        rec(if i % 2 == 0 {
            EngineError::Transport("boom".to_string())
        } else {
            EngineError::Parse("bad html".to_string())
        });
    }
    assert_eq!(
        tracker.health_row(&id).unwrap().breaker,
        BreakerState::Closed
    );
    rec(EngineError::Timeout);
    for _ in 0..4 {
        rec(EngineError::Parse("bad html".to_string()));
    }
    let row = tracker.health_row(&id).unwrap();
    assert_eq!(row.failures, 9);
    assert_eq!(
        row.breaker,
        BreakerState::Closed,
        "a `Timeout` between `Parse`s breaks the degraded streak"
    );
    rec(EngineError::Parse("bad html".to_string()));
    assert_eq!(
        tracker.health_row(&id).unwrap().breaker,
        BreakerState::Open,
        "the 5th consecutive `Parse`/`Transport` opens it"
    );
}

/// The probe's re-open window follows the failing error kind: a
/// `Parse`/`Transport` probe failure re-opens for `degraded_window`,
/// not `timeout_window`.
#[tokio::test]
async fn probe_failure_reopens_for_degraded_window() {
    let dir = tempfile::tempdir().unwrap();
    write_bad_cassette(dir.path(), "drifted selector");
    let engine = Arc::new(GateEngine::new(replay_at(dir.path(), |_| {}), true));
    let store = Arc::new(StubStore::default());
    let pipe =
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_health_policy(HealthPolicy {
            degraded_window: Duration::from_millis(80),
            timeout_window: Duration::from_secs(60),
            ..HealthPolicy::default()
        });

    // Five consecutive `Parse`s open the breaker for `degraded_window`.
    for _ in 0..5 {
        pipe.search(&req("drifted selector")).await.unwrap_err();
    }
    assert_eq!(engine.call_count(), 5);

    // Past the window the probe runs, fails with `Parse`, and re-opens.
    tokio::time::sleep(Duration::from_millis(100)).await;
    pipe.search(&req("drifted selector")).await.unwrap_err();
    assert_eq!(engine.call_count(), 6, "one probe after the window");
    let row = pipe.health().health_row(&EngineId::from("replay")).unwrap();
    assert_eq!(row.breaker, BreakerState::Open);
    let until = row.breaker_until.expect("re-opened breaker has a window");
    // `degraded_window` (80 ms), not `timeout_window` (60 s): a re-open
    // mapped to the wrong kind would sit ~60 s out.
    assert!(
        until < chrono::Utc::now() + chrono::Duration::seconds(10),
        "re-open window should be ~80 ms, got {until}"
    );
}

/// EWMA `alpha = 0.3`: the first sample seeds the estimate, later samples
/// blend `0.3 * sample + 0.7 * ewma`.
#[tokio::test]
async fn ewma_seeds_then_blends_at_alpha_point_three() {
    let store = Arc::new(StubStore::default());
    let tracker = HealthTracker::new(store);
    let id = EngineId::from("e");

    tracker.record_ok(&id, Duration::from_millis(100), Uuid::now_v7());
    assert_eq!(tracker.health_row(&id).unwrap().ewma_ms, 100.0);

    tracker.record_ok(&id, Duration::from_millis(200), Uuid::now_v7());
    let ewma = tracker.health_row(&id).unwrap().ewma_ms;
    assert!(
        (ewma - 130.0).abs() < 1e-9,
        "0.3*200 + 0.7*100 = 130, got {ewma}"
    );

    tracker.record_ok(&id, Duration::from_millis(0), Uuid::now_v7());
    let ewma = tracker.health_row(&id).unwrap().ewma_ms;
    assert!(
        (ewma - 91.0).abs() < 1e-9,
        "0.3*0 + 0.7*130 = 91, got {ewma}"
    );
}

/// Routine rows flush at most once per second; a breaker transition
/// flushes immediately. `NoResults` counts as a healthy answer.
#[tokio::test]
async fn persistence_is_debounced_and_transitions_urgent() {
    let dir = tempfile::tempdir().unwrap();
    let engine = replay_at(dir.path(), |_| {});
    let store = Arc::new(StubStore::default());
    let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

    // First network search: dirty -> due (never flushed) -> one write.
    pipe.search(&req("d1")).await.unwrap();
    assert_eq!(store.health_writes.lock().unwrap().len(), 1);

    // Two more within the debounce window: no new writes.
    pipe.search(&req("d2")).await.unwrap();
    pipe.search(&req("d3")).await.unwrap();
    assert_eq!(
        store.health_writes.lock().unwrap().len(),
        1,
        "routine updates are debounced to 1/s"
    );

    // Past the debounce interval the next search flushes again.
    tokio::time::sleep(Duration::from_millis(1_050)).await;
    pipe.search(&req("d4")).await.unwrap();
    assert_eq!(store.health_writes.lock().unwrap().len(), 2);
    let rows = store.health_rows.lock().unwrap();
    let row = rows.get("replay").expect("persisted row");
    assert_eq!(row.breaker, BreakerState::Closed);
    assert!(row.ewma_ms >= 0.0);
    assert!(row.last_ok_at.is_some());
}

/// `load` restores persisted rows: a still-open breaker stays skipped, an
/// elapsed window admits a probe, and `reset` returns a fresh HalfOpen row.
#[tokio::test]
async fn load_restores_persisted_state() {
    let store = Arc::new(StubStore::default());
    let open_id = EngineId::from("open-eng");
    let stale_id = EngineId::from("stale-eng");
    store
        .put_health(&cauce_core::EngineHealthRow {
            engine: open_id.clone(),
            ewma_ms: 500.0,
            failures: 4,
            breaker: BreakerState::Open,
            breaker_until: Some(chrono::Utc::now() + chrono::Duration::minutes(10)),
            last_ok_at: None,
            last_error: Some("rate limited by upstream".to_string()),
        })
        .await
        .unwrap();
    store
        .put_health(&cauce_core::EngineHealthRow {
            engine: stale_id.clone(),
            ewma_ms: 42.0,
            failures: 7,
            breaker: BreakerState::Open,
            breaker_until: Some(chrono::Utc::now() - chrono::Duration::seconds(1)),
            last_ok_at: None,
            last_error: Some("timed out".to_string()),
        })
        .await
        .unwrap();

    let tracker = HealthTracker::new(store.clone());
    assert_eq!(tracker.load().await.unwrap(), 2);

    // Still inside the window: skipped, persisted fields intact.
    assert_eq!(tracker.admission(&open_id, Uuid::now_v7()), Gate::Skip);
    let row = tracker.health_row(&open_id).unwrap();
    assert_eq!(row.ewma_ms, 500.0);
    assert_eq!(row.failures, 4);
    assert_eq!(row.last_error.as_deref(), Some("rate limited by upstream"));

    // Elapsed window: one probe admitted, a second is not.
    assert_eq!(tracker.admission(&stale_id, Uuid::now_v7()), Gate::Probe);
    assert_eq!(
        tracker.admission(&stale_id, Uuid::now_v7()),
        Gate::Skip,
        "half-open admits exactly one probe"
    );

    // Reset restores a fresh HalfOpen row (the route persists + audits
    // it): the next call is admitted as the single probe (W2-05).
    let (previous, row) = tracker.reset(&open_id).expect("known engine");
    assert_eq!(previous, BreakerState::Open);
    assert_eq!(row.breaker, BreakerState::HalfOpen);
    assert_eq!(row.failures, 0);
    assert_eq!(row.ewma_ms, 0.0);
    assert_eq!(tracker.admission(&open_id, Uuid::now_v7()), Gate::Probe);
    assert!(tracker.reset(&EngineId::from("unknown")).is_none());
}
