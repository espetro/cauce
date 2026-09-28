//! Breaker recovery prober (issue #227): active `HalfOpen` probing so an
//! engine returns to service without inbound traffic or a manual reset.
//!
//! [`SearchPipeline::probe_due`] runs one pass — for every configured
//! engine it asks [`HealthTracker::admission`] for the gate; a `Probe`
//! claim (an `Open` whose `breaker_until` elapsed, or an idle
//! `HalfOpen`) spawns the engine call through the same
//! [`SearchPipeline::spawn_engine_calls`] path the fan-out uses, so
//! health recording, metrics, the single-probe gate and the probe guard
//! are identical to a request-driven probe. Rate is capped by the
//! tracker itself: one probe per engine per re-open window, and the
//! adaptive backoff (`HealthPolicy::probe_window_*`) stretches the
//! interval for engines that keep failing.
//!
//! [`SearchPipeline::spawn_prober`] is the `cauce serve` loop: every
//! `health.probe_tick` it runs a pass and flushes the tracker, which is
//! what keeps the no-traffic persistence story honest — a prober-driven
//! transition reaches `engine_health`/`engine.breaker` audit rows
//! without waiting for a search to trigger `flush_due`.
//!
//! [`HealthTracker::admission`]: crate::health::HealthTracker::admission
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::health::Gate;
use crate::request::{ClientKind, SafeSearch, SearchRequest};

use super::waves::Gated;
use super::*;

/// The query probes issue (issue #227): a fixed, innocuous page-1 query
/// — the same role SearXNG's health-check queries fill. It exercises
/// the real upstream path (DNS, TLS, HTTP, parse) without touching
/// `search_log` or the response cache because the call never enters
/// `SearchPipeline::search`. A `NoResults` answer is still an answer:
/// it closes the breaker like any healthy response.
const PROBE_QUERY: &str = "cauce health probe";

fn probe_request() -> SearchRequest {
    SearchRequest {
        q: PROBE_QUERY.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: None,
        client: ClientKind::Api,
    }
}

impl SearchPipeline {
    /// One recovery pass: claim every engine whose breaker admits a
    /// probe right now (the same lazy `Open -> HalfOpen` transition a
    /// request would trigger) and run its probe on the regular spawn
    /// path under the pipeline deadline. Returns the number of probes
    /// launched. Awaiting the pass keeps ticks from overlapping and
    /// lets a caller sequence assertions deterministically.
    pub async fn probe_due(&self) -> usize {
        let request_id = Uuid::now_v7();
        let mut claimed: Vec<Gated> = Vec::new();
        for (idx, engine) in self.engines.iter().enumerate() {
            if self.health.admission(&engine.id(), request_id) == Gate::Probe {
                claimed.push((idx, engine.clone()));
            }
        }
        if claimed.is_empty() {
            return 0;
        }
        info!(engines = claimed.len(), "breaker recovery: probing engines");
        let req = probe_request();
        let ids: Vec<EngineId> = claimed.iter().map(|(_, e)| e.id()).collect();
        let mut set = self.spawn_engine_calls(&req, &claimed, request_id, self.deadline);
        let mut done = 0usize;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(_) => done += 1,
                Err(e) => warn!(error = %e, "probe task failed to join"),
            }
        }
        // Refresh the breaker gauge for the probed engines — requests
        // would do this on their next gate, but the whole point of the
        // prober is recovery without traffic.
        for id in ids {
            if let Some(row) = self.health.health_row(&id) {
                self.metrics.record_breaker_state(&id, row.breaker);
            }
        }
        done
    }

    /// The `cauce serve` recovery loop: every `health.probe_tick`, run a
    /// [`probe_due`](Self::probe_due) pass and flush the tracker so
    /// prober-driven transitions persist promptly. `None` when
    /// `probe_tick` is `Duration::ZERO` (probing stays passive — only
    /// inbound requests drive it).
    pub fn spawn_prober(&self) -> Option<tokio::task::JoinHandle<()>> {
        let tick = self.health.probe_tick();
        if tick.is_zero() {
            debug!("breaker recovery prober disabled (probe_tick = 0)");
            return None;
        }
        let pipe = self.clone();
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let n = pipe.probe_due().await;
                if let Err(e) = pipe.health().flush_due().await {
                    warn!(error = %e, "health flush after probe pass failed");
                }
                if n > 0 {
                    debug!(probes = n, "breaker recovery pass done");
                }
            }
        }))
    }
}
