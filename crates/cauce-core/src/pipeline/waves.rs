//! W1-06 breaker gating and W3-01 primary/deferred wave construction.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::info;
use uuid::Uuid;

use crate::admission::EnginePermits;
use crate::engine::{Engine, EngineId, Tier};
use crate::health::Gate;
use crate::store::BreakerState;

use super::*;

/// One gated engine plus its index in the request's runnable list, so
/// merge tie-breaks and `engines_used` keep configured order even though
/// the hedge wave spawns late (W3-01).
pub(super) type Gated = (usize, Arc<dyn Engine>);

/// The fan-out's two waves (W3-01): `gated` runs at t=0 (tier-1 plus
/// specialised tier-3), `deferred` is the tier-2 hedge set the scheduler
/// fires at the hedge point — gated then, so a claimed `HalfOpen` probe
/// always precedes a real call. `skipped` accumulates the ids both gates
/// suppress (`meta.engines_skipped`).
pub(super) struct Waves {
    pub(super) gated: Vec<Gated>,
    pub(super) deferred: Vec<Gated>,
    pub(super) skipped: Vec<EngineId>,
}

impl SearchPipeline {
    /// The breaker gate (W1-06), extracted so `fetch` and `fetch_stream`
    /// share it verbatim: `Open` engines are skipped, `HalfOpen` admits
    /// exactly one probe. Gated here — after the permit wait, immediately
    /// before a wave spawns — so a claimed probe is always followed by
    /// its call: the tier-2 hit and overflow paths return before this
    /// point and never consume the single probe slot, and the W3-01
    /// deferred wave only gates at fire time. Returns the admitted
    /// engines (with their runnable indices) and the ids the open
    /// breakers suppressed.
    pub(super) fn breaker_gate(
        &self,
        runnable: &[Gated],
        request_id: Uuid,
    ) -> (Vec<Gated>, Vec<EngineId>) {
        let mut gated: Vec<Gated> = Vec::with_capacity(runnable.len());
        let mut skipped: Vec<EngineId> = Vec::new();
        for (idx, engine) in runnable {
            match self.health.admission(&engine.id(), request_id) {
                Gate::Call | Gate::Probe => gated.push((*idx, engine.clone())),
                Gate::Skip => skipped.push(engine.id()),
            }
            // `cauce_engine_breaker_state` mirrors the tracker's live row:
            // `admission` may have just flipped `Open` -> `HalfOpen`.
            let state = self
                .health
                .health_row(&engine.id())
                .map(|row| row.breaker)
                .unwrap_or(BreakerState::Closed);
            self.metrics.record_breaker_state(&engine.id(), state);
        }
        if !skipped.is_empty() {
            info!(
                skipped = render_ids(&skipped),
                "engines skipped by open breaker"
            );
        }
        (gated, skipped)
    }

    /// Split `runnable` into the fan-out's two waves and breaker-gate the
    /// t=0 one (W3-01): tier-1 and specialised tier-3 engines run
    /// immediately; tier-2 is the hedge set, gated only once its permits
    /// are held — by [`SearchPipeline::promote_deferred`] when the t=0
    /// wave is empty, or by [`SearchPipeline::finish_hedge`] when a hedge
    /// trigger fires — so a claimed `HalfOpen` probe always precedes a
    /// real call. Returns the waves and the hedge point (`None` when the
    /// t=0 wave is empty: the deferred wave promotes instead of hedging).
    pub(super) fn gate_waves(
        &self,
        runnable: &[Arc<dyn Engine>],
        request_id: Uuid,
    ) -> (Waves, Option<Duration>) {
        let (primary, deferred): (Vec<Gated>, Vec<Gated>) = runnable
            .iter()
            .cloned()
            .enumerate()
            .partition(|(_, e)| e.tier() != Tier::T2);
        let (gated, skipped) = self.breaker_gate(&primary, request_id);
        // P90 pools only the tier-1 engines this request actually
        // admitted: a skipped engine cannot answer, so its slow history
        // must not postpone the hedge for the healthy set.
        let hedge_at = if gated.is_empty() {
            None
        } else {
            self.hedge_point(&gated, &deferred)
        };
        (
            Waves {
                gated,
                deferred,
                skipped,
            },
            hedge_at,
        )
    }

    /// `t = clamp(P90(tier-1 history), floor, ceiling)` for this flight
    /// (W3-01), `None` when the hedge has nothing to fire. An empty
    /// history yields `P90 = 0`, which the floor turns into the earliest
    /// hedge point.
    pub(super) fn hedge_point(&self, primary: &[Gated], deferred: &[Gated]) -> Option<Duration> {
        if deferred.is_empty() {
            return None;
        }
        let t1: Vec<EngineId> = primary
            .iter()
            .filter(|(_, e)| e.tier() == Tier::T1)
            .map(|(_, e)| e.id())
            .collect();
        // `Duration::clamp` panics on `min > max`; config validation
        // rejects that, but a hand-built `HedgePolicy` must not kill
        // engine tasks, so order the bounds here regardless.
        let lo = self.hedge.floor.min(self.hedge.ceiling);
        let hi = self.hedge.floor.max(self.hedge.ceiling);
        Some(Duration::from_millis(self.health.p90_ms(&t1)).clamp(lo, hi))
    }

    /// Promote the deferred wave when the t=0 gate left nothing
    /// runnable (every primary absent or breaker-skipped): acquire the
    /// wave's permits BEFORE breaker-gating it — a claimed `HalfOpen`
    /// probe must always precede a spawn, and a failed wait must leave
    /// no claims behind — then move the survivors into `waves.gated`.
    /// `Ok(None)` when the primary wave already has work.
    /// `Err(RateLimited)` when tier-2 capacity never frees inside
    /// `min(remaining deadline, max_wait)` — the caller routes that to
    /// the same overflow fallback as an exhausted primary queue — and
    /// `Err(BreakerOpen)` when nothing at all survived the gates.
    pub(super) async fn promote_deferred(
        &self,
        waves: &mut Waves,
        started: Instant,
        request_id: Uuid,
    ) -> Result<Option<EnginePermits>, PipelineError> {
        if !waves.gated.is_empty() {
            return Ok(None);
        }
        if waves.deferred.is_empty() {
            // Nothing ran; `shared_response` still writes this request's
            // search_log row on the error path.
            return Err(PipelineError::BreakerOpen(std::mem::take(
                &mut waves.skipped,
            )));
        }
        let ids: Vec<EngineId> = waves.deferred.iter().map(|(_, e)| e.id()).collect();
        // Same bound as the upfront queue (`admission.max_wait`) but
        // never past the hard deadline.
        let wait = self
            .deadline
            .saturating_sub(started.elapsed())
            .min(self.admission.limits().max_wait);
        let permits = self
            .admission
            .acquire_within(&ids, wait)
            .await
            .map_err(|_| self.rate_limited())?;
        let (now, now_skipped) = self.breaker_gate(&waves.deferred, request_id);
        waves.skipped.extend(now_skipped);
        waves.deferred.clear();
        waves.gated = now;
        if waves.gated.is_empty() {
            return Err(PipelineError::BreakerOpen(std::mem::take(
                &mut waves.skipped,
            )));
        }
        Ok(Some(permits))
    }
}
