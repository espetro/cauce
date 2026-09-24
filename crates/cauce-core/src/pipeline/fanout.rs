//! Engine fan-out: wave spawning, hedge permit race, deadline enforcement.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{Instrument, debug, info, info_span, warn};
use uuid::Uuid;

use crate::admission::{EnginePermits, WaitTimeout};
use crate::cache::CacheKey;
use crate::engine::{Engine, EngineError, EngineId};
use crate::metrics::engine_error_label;
use crate::normalize::normalize_url;
use crate::request::SearchRequest;
use crate::response::{
    EngineReport, EngineStatus, SearchResponse, SearchResult, StreamEvent, StreamMeta,
};

use super::merge::RrfMerge;
use super::waves::{Gated, Waves};
use super::*;

/// One completed engine call. `idx` is the position in the runnable list
/// so duplicate engine ids (two `replay` instances) stay distinguishable.
pub(super) struct EngineOutcome {
    pub(super) idx: usize,
    pub(super) id: EngineId,
    pub(super) latency: Duration,
    /// `Err(Elapsed)` means the hard deadline fired, not the engine.
    pub(super) outcome: Result<Result<Vec<SearchResult>, EngineError>, tokio::time::error::Elapsed>,
}

/// Fan-out bookkeeping shared by the collect and incremental stream paths.
pub(super) struct FanOut {
    pub(super) answered: Vec<bool>,
    pub(super) reports: Vec<(usize, EngineReport)>,
    pub(super) failures: Vec<(usize, EngineId, EngineError)>,
    pub(super) deadline_hit: bool,
    pub(super) had_answer: bool,
    pub(super) ttfr_recorded: bool,
    /// W3-01: the hedge fired tier-2 engines this flight.
    pub(super) hedged: bool,
    /// Elapsed ms at which it fired (`meta.hedge_at_ms`).
    pub(super) hedge_at_ms: Option<u32>,
    pub(super) started_at: Instant,
}

impl FanOut {
    pub(super) fn new(engines: usize, started_at: Instant) -> Self {
        Self {
            answered: vec![false; engines],
            reports: Vec::with_capacity(engines),
            failures: Vec::new(),
            deadline_hit: false,
            had_answer: false,
            ttfr_recorded: false,
            hedged: false,
            hedge_at_ms: None,
            started_at,
        }
    }
}

impl SearchPipeline {
    /// One flight's upstream work: parallel fan-out under the hard
    /// deadline, per-engine reports, RRF merge, persist, response. The
    /// `search_log` write is deliberately absent — each waiter writes its
    /// own row in [`SearchPipeline::shared_response`].
    pub(super) async fn fetch(
        &self,
        req: &SearchRequest,
        runnable: &[Arc<dyn Engine>],
        key: &CacheKey,
        ttl: Duration,
        request_id: Uuid,
        started: Instant,
    ) -> Result<SearchResponse, PipelineError> {
        let (mut waves, hedge_at) = self.gate_waves(runnable, request_id);
        let _promoted = self
            .promote_deferred(&mut waves, started, request_id)
            .await?;
        let mut fan = FanOut::new(runnable.len(), started);
        let mut merge = RrfMerge::new();
        let ctx = FetchCtx {
            req,
            key,
            ttl,
            request_id,
            started,
        };
        self.drive_fan_out(&ctx, &mut waves, hedge_at, &mut fan, &mut merge, |_, _| {})
            .await;
        let merged = self.merge_outcomes(&waves.gated, &mut fan, merge, request_id);
        self.finish_fetch(waves.skipped, fan, merged, ctx).await
    }

    /// The streaming fan-out (W2-01): identical stages to
    /// [`SearchPipeline::fetch`] — breaker gate, per-engine reports,
    /// health, merge, persist — but each engine's page is pushed as a
    /// `results` event the moment its call resolves, merge state is kept
    /// incrementally in an [`RrfMerge`], and the terminal `meta` event
    /// carries the final RRF `order` (the merged rows' dedupe keys, in the
    /// same order `resp.results` holds).
    pub(super) async fn fetch_stream(
        &self,
        stream: &StreamCtx<'_>,
        runnable: &[Arc<dyn Engine>],
    ) -> Result<SearchResponse, PipelineError> {
        let (mut waves, hedge_at) = self.gate_waves(runnable, stream.request_id);
        let _promoted = self
            .promote_deferred(&mut waves, stream.started, stream.request_id)
            .await?;
        let mut fan = FanOut::new(runnable.len(), stream.started);
        let mut merge = RrfMerge::new();
        let ctx = FetchCtx {
            req: stream.req,
            key: stream.key,
            ttl: stream.ttl,
            request_id: stream.request_id,
            started: stream.started,
        };
        self.drive_fan_out(
            &ctx,
            &mut waves,
            hedge_at,
            &mut fan,
            &mut merge,
            |engine, results| {
                let _ = stream.tx.send(StreamEvent::Results {
                    engine,
                    results,
                    elapsed_ms: millis(stream.started.elapsed()),
                });
            },
        )
        .await;
        let merged = self.merge_outcomes(&waves.gated, &mut fan, merge, stream.request_id);
        let resp = self.finish_fetch(waves.skipped, fan, merged, ctx).await?;
        let _ = stream.tx.send(StreamEvent::Meta(StreamMeta {
            meta: resp.meta.clone(),
            order: resp.results.iter().map(|r| normalize_url(&r.url)).collect(),
        }));
        Ok(resp)
    }

    /// Fold one [`EngineOutcome`] into the [`FanOut`] bookkeeping: metrics,
    /// health, the per-engine report, and (for answers) the result list.
    /// Returns `Some(results)` when the engine answered — an empty vec for
    /// `NoResults`, which is an answer, not a failure: the engine
    /// responded and there is simply no page to serve (the exec protocol's
    /// first-class code; `replay` uses it for pages beyond `page_limit`;
    /// an `Ok` empty page normalizes to it in `fan_out`, issue #120). It
    /// counts toward "an engine answered" so an all-`NoResults` fan-out is
    /// a 200-shaped empty response, not `AllEnginesFailed` (v2's "page 2
    /// always 502" defect). The report stays `Failed(NoResults)` for
    /// honesty.
    pub(super) fn fold_outcome(
        &self,
        fan: &mut FanOut,
        outcome: EngineOutcome,
        started: Instant,
    ) -> Option<Vec<SearchResult>> {
        let idx = outcome.idx;
        fan.answered[idx] = true;
        let latency_ms = millis(outcome.latency);
        match outcome.outcome {
            Ok(Ok(results)) => {
                fan.had_answer = true;
                self.metrics.record_engine_call(
                    &outcome.id,
                    "ok",
                    outcome.latency,
                    Some(results.len()),
                );
                if !fan.ttfr_recorded {
                    // Time to first engine result, measured from the
                    // search start (includes the cache-miss lookup).
                    self.metrics.record_ttfr(started.elapsed());
                    fan.ttfr_recorded = true;
                }
                fan.reports.push((
                    idx,
                    EngineReport {
                        engine: outcome.id.clone(),
                        status: EngineStatus::Ok,
                        latency_ms,
                        result_count: results.len() as u32,
                    },
                ));
                Some(results)
            }
            Ok(Err(EngineError::NoResults)) => {
                // A completed call with an empty answer.
                fan.had_answer = true;
                self.metrics.record_engine_call(
                    &outcome.id,
                    "no_results",
                    outcome.latency,
                    Some(0),
                );
                fan.reports.push((
                    idx,
                    EngineReport {
                        engine: outcome.id.clone(),
                        status: EngineStatus::Failed(EngineError::NoResults),
                        latency_ms,
                        result_count: 0,
                    },
                ));
                Some(Vec::new())
            }
            Ok(Err(err)) => {
                self.metrics.record_engine_call(
                    &outcome.id,
                    engine_error_label(&err),
                    outcome.latency,
                    None,
                );
                fan.reports.push((
                    idx,
                    EngineReport {
                        engine: outcome.id.clone(),
                        status: EngineStatus::Failed(err.clone()),
                        latency_ms,
                        result_count: 0,
                    },
                ));
                fan.failures.push((idx, outcome.id, err));
                None
            }
            Err(_elapsed) => {
                fan.deadline_hit = true;
                self.metrics
                    .record_engine_call(&outcome.id, "timeout", outcome.latency, None);
                fan.reports.push((
                    idx,
                    EngineReport {
                        engine: outcome.id.clone(),
                        status: EngineStatus::Failed(EngineError::Timeout),
                        latency_ms,
                        result_count: 0,
                    },
                ));
                fan.failures.push((idx, outcome.id, EngineError::Timeout));
                None
            }
        }
    }

    /// Mark panicked/cancelled engine tasks as transport failures. A JoinError
    /// does not carry its engine id, so unanswered slots identify them.
    pub(super) fn reconcile_unanswered(&self, gated: &[Gated], fan: &mut FanOut, request_id: Uuid) {
        for (idx, engine) in gated {
            let idx = *idx;
            if fan.answered[idx] {
                continue;
            }
            let id = engine.id();
            let elapsed = fan.started_at.elapsed();
            let err = EngineError::Transport("engine task failed".to_string());
            self.metrics
                .record_engine_call(&id, "transport", elapsed, None);
            self.health.record_err(&id, elapsed, &err, request_id);
            fan.reports.push((
                idx,
                EngineReport {
                    engine: id.clone(),
                    status: EngineStatus::Failed(err.clone()),
                    latency_ms: millis(elapsed),
                    result_count: 0,
                },
            ));
            fan.failures.push((idx, id, err));
        }
    }

    /// The hedged fan-out loop shared by `fetch` and `fetch_stream`
    /// (W3-01): joins engine tasks as they answer, folds each outcome
    /// (metrics, health, report, incremental merge), and at the hedge
    /// point — or earlier, once every primary call answered short —
    /// fires the healthy tier-2 set on the remaining budget while fewer
    /// than `hedge.min_results` merged results have arrived. `on_results`
    /// receives each answering engine's non-empty page (the stream path
    /// emits it as a `results` event; the collect path discards it).
    ///
    /// Two instants matter separately: `ctx.started` (request start —
    /// the hard deadline and `elapsed_ms` measure from it, so engine
    /// budgets shrink by whatever pre-fan-out work consumed) and
    /// `fan_started` (the moment the primary wave spawned — the hedge
    /// timer and `meta.hedge_at_ms` measure from it, so pre-fan-out
    /// work cannot eat the floor tier-1 was promised).
    ///
    /// A hedge trigger does not spawn directly: the deferred wave's
    /// permits are acquired inside this loop (raced against incoming
    /// outcomes) so tier-2 capacity is only ever reserved while a hedge
    /// wave is actually about to run. A permit wait cancelled by a
    /// filling merge or lost to the deadline simply means no hedge —
    /// the response reports `hedged: false` either way.
    pub(super) async fn drive_fan_out(
        &self,
        ctx: &FetchCtx<'_>,
        waves: &mut Waves,
        hedge_at: Option<Duration>,
        fan: &mut FanOut,
        merge: &mut RrfMerge,
        mut on_results: impl FnMut(EngineId, Vec<SearchResult>),
    ) {
        let fan_started = Instant::now();
        let hard_deadline = ctx.started + self.deadline;
        let mut set = self.spawn_engine_calls(
            ctx.req,
            &waves.gated,
            ctx.request_id,
            self.deadline.saturating_sub(ctx.started.elapsed()),
        );
        let mut hedge_pending = hedge_at.is_some();
        // The hedge timer counts from fan-out start (not request start)
        // and is capped at the hard deadline: a hedge point past it has
        // no spawn budget left anyway.
        let hedge_wake = hedge_at
            .map(|at| tokio::time::Instant::from_std((fan_started + at).min(hard_deadline)));
        let mut hedge_acquire: Option<tokio::task::JoinHandle<Result<EnginePermits, WaitTimeout>>> =
            None;
        // Held until the loop ends so the spawned hedge wave's calls
        // stay covered — same role as the caller's `_permits` binding.
        let mut _held_hedge_permits: Option<EnginePermits> = None;
        loop {
            if set.is_empty() && !hedge_pending && hedge_acquire.is_none() {
                break;
            }
            tokio::select! {
                joined = set.join_next(), if !set.is_empty() => {
                    match joined {
                        Some(Ok(outcome)) => {
                            let idx = outcome.idx;
                            let engine = outcome.id.clone();
                            if let Some(results) = self.fold_outcome(fan, outcome, ctx.started)
                                && !results.is_empty()
                            {
                                merge.add(idx, &results);
                                on_results(engine, results);
                            }
                            // Early resolve: every primary call answered.
                            // A short merged page fires the hedge now
                            // rather than at the hedge point; a full one
                            // cancels it.
                            if hedge_pending
                                && waves
                                    .gated
                                    .iter()
                                    .all(|(idx, _)| fan.answered[*idx])
                            {
                                hedge_pending = false;
                                if merge.map.len() < self.hedge.min_results {
                                    hedge_acquire = self.queue_hedge(waves, ctx);
                                }
                            }
                            // A merge that filled while tier-2 permits
                            // were still queued makes the hedge moot.
                            if hedge_acquire.is_some()
                                && merge.map.len() >= self.hedge.min_results
                            {
                                hedge_acquire.take().unwrap().abort();
                            }
                        }
                        Some(Err(join_err)) => {
                            warn!(error = %join_err, "engine task failed to join")
                        }
                        None => {}
                    }
                }
                _ = async {
                    match hedge_wake {
                        Some(t) => tokio::time::sleep_until(t).await,
                        None => std::future::pending().await,
                    }
                }, if hedge_pending => {
                    hedge_pending = false;
                    // `hedge_wake` caps at the hard deadline, so an
                    // elapsed deadline means nothing can spawn — the
                    // loop then ends on the last task timeout.
                    if ctx.started.elapsed() < self.deadline
                        && merge.map.len() < self.hedge.min_results
                    {
                        hedge_acquire = self.queue_hedge(waves, ctx);
                    }
                }
                // `select!` evaluates every branch's expression even for
                // disabled branches, so this cannot unwrap — a `None`
                // acquire yields a pending future the poll never uses.
                resolved = async {
                    match hedge_acquire.as_mut() {
                        Some(handle) => handle.await,
                        None => std::future::pending().await,
                    }
                }, if hedge_acquire.is_some() => {
                    hedge_acquire = None;
                    match resolved {
                        Ok(Ok(permits)) => {
                            _held_hedge_permits = Some(permits);
                            self.finish_hedge(
                                &mut set,
                                waves,
                                fan,
                                ctx,
                                fan_started,
                            );
                        }
                        Ok(Err(_timeout)) => {
                            info!(
                                engines = waves.deferred.len(),
                                "hedge dropped: tier-2 permits never freed"
                            );
                        }
                        Err(join_err) => {
                            warn!(error = %join_err, "hedge permit task failed");
                        }
                    }
                }
            }
        }
    }

    /// Begin a hedge (W3-01): spawn the task that waits for the
    /// deferred wave's permits, bounded by the deadline budget remaining
    /// at trigger time. The wave is NOT breaker-gated here — gating
    /// claims `HalfOpen` probes, and a claim must always precede a
    /// spawn, so it happens inside [`SearchPipeline::finish_hedge`] once
    /// the permits are actually held (an aborted or timed-out wait then
    /// leaves no claims behind). `None` when nothing is deferred or the
    /// deadline already elapsed — a spawn would get a zero budget anyway.
    pub(super) fn queue_hedge(
        &self,
        waves: &Waves,
        ctx: &FetchCtx<'_>,
    ) -> Option<tokio::task::JoinHandle<Result<EnginePermits, WaitTimeout>>> {
        let remaining = self.deadline.saturating_sub(ctx.started.elapsed());
        if remaining.is_zero() || waves.deferred.is_empty() {
            return None;
        }
        let ids: Vec<EngineId> = waves.deferred.iter().map(|(_, e)| e.id()).collect();
        let admission = self.admission.clone();
        Some(tokio::spawn(async move {
            admission.acquire_within(&ids, remaining).await
        }))
    }

    /// Fire the hedge once its permits are held (W3-01): re-check the
    /// deadline — a permit resolved past it must not gate or spawn —
    /// then breaker-gate the deferred wave (suppressions land on
    /// `engines_skipped` like the t=0 gate's), mark
    /// `meta.hedged`/`meta.hedge_at_ms` (measured from fan-out start)
    /// and `cauce_hedge_total{reason}` — `few` once every primary call
    /// answered, `slow` while one is still in flight — and spawn the
    /// survivors on the deadline budget remaining at fire time.
    pub(super) fn finish_hedge(
        &self,
        set: &mut tokio::task::JoinSet<EngineOutcome>,
        waves: &mut Waves,
        fan: &mut FanOut,
        ctx: &FetchCtx<'_>,
        fan_started: Instant,
    ) {
        let remaining = self.deadline.saturating_sub(ctx.started.elapsed());
        if remaining.is_zero() {
            info!("hedge dropped: deadline elapsed during the permit wait");
            return;
        }
        let (now, now_skipped) = self.breaker_gate(&waves.deferred, ctx.request_id);
        waves.skipped.extend(now_skipped);
        waves.deferred.clear();
        if now.is_empty() {
            debug!("hedge point reached but every tier-2 engine is breaker-skipped");
            return;
        }
        let reason = if waves.gated.iter().all(|(idx, _)| fan.answered[*idx]) {
            "few"
        } else {
            "slow"
        };
        let hedge_at = fan_started.elapsed();
        self.metrics.record_hedge(reason);
        fan.hedged = true;
        fan.hedge_at_ms = Some(millis(hedge_at));
        info!(
            hedge_at_ms = millis(hedge_at),
            reason,
            engines = now.len(),
            "hedged to tier-2"
        );
        self.spawn_into(set, ctx.req, &now, ctx.request_id, remaining);
        waves.gated.extend(now);
    }

    /// Spawn one bounded-deadline task per gated engine. Both waves share
    /// this path so streaming cannot diverge in health or error rules:
    /// the t=0 wave gets the full deadline, the W3-01 hedge wave the
    /// budget remaining at fire time.
    pub(super) fn spawn_engine_calls(
        &self,
        req: &SearchRequest,
        gated: &[Gated],
        request_id: Uuid,
        budget: Duration,
    ) -> tokio::task::JoinSet<EngineOutcome> {
        let mut set = tokio::task::JoinSet::new();
        self.spawn_into(&mut set, req, gated, request_id, budget);
        set
    }

    /// [`spawn_engine_calls`] for a `JoinSet` that already exists — the
    /// hedge wave joining the flight's set late (W3-01).
    pub(super) fn spawn_into(
        &self,
        set: &mut tokio::task::JoinSet<EngineOutcome>,
        req: &SearchRequest,
        gated: &[Gated],
        request_id: Uuid,
        budget: Duration,
    ) {
        for (idx, engine) in gated {
            let idx = *idx;
            let engine = engine.clone();
            let id = engine.id();
            let deadline = budget;
            let span = info_span!(
                "engine",
                request_id = %request_id,
                engine = %id,
                tier = engine.tier().as_u8(),
                status = tracing::field::Empty,
                results = tracing::field::Empty,
            );
            let req2 = req.clone();
            let recorder = span.clone();
            let health = self.health.clone();
            set.spawn(
                async move {
                    let _probe_guard = health.probe_guard(&id);
                    let t0 = Instant::now();
                    let outcome = tokio::time::timeout(deadline, engine.search(&req2, deadline))
                        .await
                        .map(|done| match done {
                            // Normalize empty successes across engine runtimes.
                            Ok(results) if results.is_empty() => Err(EngineError::NoResults),
                            done => done,
                        });
                    let latency = t0.elapsed();
                    match &outcome {
                        Ok(Ok(r)) => {
                            health.record_ok(&id, latency, request_id);
                            recorder.record("status", "ok");
                            recorder.record("results", r.len() as u64);
                            debug!(results = r.len(), "engine done");
                        }
                        Ok(Err(e @ EngineError::NoResults)) => {
                            health.record_ok(&id, latency, request_id);
                            recorder.record("status", "error");
                            recorder.record("results", 0u64);
                            debug!(error = %e, "engine failed");
                        }
                        Ok(Err(e)) => {
                            health.record_err(&id, latency, e, request_id);
                            recorder.record("status", "error");
                            recorder.record("results", 0u64);
                            debug!(error = %e, "engine failed");
                        }
                        Err(_) => {
                            health.record_err(&id, latency, &EngineError::Timeout, request_id);
                            recorder.record("status", "timeout");
                            recorder.record("results", 0u64);
                            debug!("engine deadline exceeded");
                        }
                    }
                    EngineOutcome {
                        idx,
                        id,
                        latency,
                        outcome,
                    }
                }
                .instrument(span),
            );
        }
    }
}
