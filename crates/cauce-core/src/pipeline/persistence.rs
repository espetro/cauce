//! Shared response shaping, cache persistence and the search log.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tracing::{Instrument, debug, info, info_span, warn};
use uuid::Uuid;

use crate::admission::FlightResult;
use crate::cache::{CacheKey, normalize_query};
use crate::engine::{Engine, EngineError, EngineId, Tier};
use crate::request::SearchRequest;
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source,
};
use crate::store::{LogSource, SearchLogRow};

use super::fanout::FanOut;
use super::*;

/// Fields of a `SearchLogRow` the call site supplies; `write_log` fills
/// `id`, `ts`, `query_hash`, `query`, `client` and `latency_ms`.
pub(super) struct LogRow {
    pub(super) source: LogSource,
    pub(super) tier: Option<Tier>,
    pub(super) result_count: u32,
    pub(super) engines: Vec<EngineId>,
    pub(super) deadline_hit: bool,
}

impl SearchPipeline {
    pub(super) fn rate_limited(&self) -> PipelineError {
        let retry_after_s = self.admission.retry_after_s();
        info!(
            admission_rejected = true,
            reason = "queue_full",
            retry_after_s,
            "admission: rejected, no row to fall back on"
        );
        PipelineError::RateLimited { retry_after_s }
    }

    /// Per-request completion of a shared outcome: rewrite `request_id`
    /// and `elapsed_ms` for this waiter, write this request's
    /// `search_log` row, return. Every waiter — leader included — lands
    /// here, so the unconditional-log rule holds per request, not per
    /// flight.
    pub(super) async fn shared_response(
        &self,
        req: &SearchRequest,
        key: &CacheKey,
        runnable: &[Arc<dyn Engine>],
        shared: FlightResult,
        request_id: Uuid,
        started: Instant,
    ) -> Result<SearchResponse, PipelineError> {
        let query = normalize_query(&req.q);
        match shared {
            Ok(resp) => {
                let mut resp = (*resp).clone();
                resp.meta.request_id = request_id;
                resp.meta.elapsed_ms = millis(started.elapsed());
                // On a cache-sourced row (the stale-serve path) the log
                // carries the producing engines (Ok provenance), matching
                // the fresh-hit path's `hit.engines`; on a network row it
                // carries every engine that ran.
                let (source, tier, engines) = match &resp.meta.source {
                    Source::Cache { tier, .. } => (
                        LogSource::Cache,
                        Some(*tier),
                        resp.meta
                            .engines_used
                            .iter()
                            .filter(|r| matches!(r.status, EngineStatus::Ok))
                            .map(|r| r.engine.clone())
                            .collect(),
                    ),
                    Source::Network => (
                        LogSource::Network,
                        None,
                        resp.meta
                            .engines_used
                            .iter()
                            .map(|r| r.engine.clone())
                            .collect(),
                    ),
                };
                let label = if matches!(resp.meta.source, Source::Cache { .. }) {
                    "cache"
                } else {
                    "network"
                };
                self.write_log(
                    req,
                    key,
                    &query,
                    LogRow {
                        source,
                        tier,
                        result_count: resp.results.len() as u32,
                        engines,
                        deadline_hit: resp.meta.deadline_hit,
                    },
                    started,
                )
                .await;
                info!(
                    source = label,
                    tier = tier.map(|t| t.as_u8()).unwrap_or_default(),
                    results = resp.results.len(),
                    elapsed_ms = resp.meta.elapsed_ms,
                    deadline_hit = resp.meta.deadline_hit,
                    "search complete"
                );
                // Per-request metrics: every waiter on the flight lands
                // here, so counters count requests, not flights. A stale
                // serve is observed by each waiter; the deadline hit was
                // already counted per flight in `fetch`.
                self.metrics
                    .record_search(&req.client, label, tier, "ok", started.elapsed());
                if matches!(resp.meta.source, Source::Cache { stale: true, .. }) {
                    self.metrics.record_stale_served();
                }
                Ok(resp)
            }
            Err(e) => {
                if matches!(e, PipelineError::AllEnginesFailed(_)) {
                    warn!(failures = e.failures().len(), "all engines failed");
                }
                self.write_log(
                    req,
                    key,
                    &query,
                    LogRow {
                        source: LogSource::Network,
                        tier: None,
                        result_count: 0,
                        engines: runnable.iter().map(|e| e.id()).collect(),
                        deadline_hit: false,
                    },
                    started,
                )
                .await;
                // W2-03 `outcome` label: the admission-rejected 429 is
                // `rejected`; every other pipeline failure (`AllEnginesFailed`,
                // `NoEngines`, `BreakerOpen`, ...) is `error`.
                let outcome = if matches!(e, PipelineError::RateLimited { .. }) {
                    "rejected"
                } else {
                    "error"
                };
                self.metrics.record_search(
                    &req.client,
                    "network",
                    None,
                    outcome,
                    started.elapsed(),
                );
                // A 429 reached the client: one rejection per waiter.
                // `queue_full` matches the reason label in `rate_limited`.
                if matches!(e, PipelineError::RateLimited { .. }) {
                    self.metrics.record_admission_rejected("queue_full");
                }
                Err(e)
            }
        }
    }

    /// Everything after the fan-out both `fetch` variants share: report
    /// ordering, deadline metric, health flush, the all-failed error,
    /// response assembly (with breaker-skipped ids in `engines_skipped`,
    /// W2-01), and the cache persist. `merged` is the final RRF list —
    /// accumulated incrementally on both paths so the W3-01 hedge can
    /// read the merged count mid-flight.
    pub(super) async fn finish_fetch(
        &self,
        skipped: Vec<EngineId>,
        mut fan: FanOut,
        merged: Vec<SearchResult>,
        ctx: FetchCtx<'_>,
    ) -> Result<SearchResponse, PipelineError> {
        // Fan-out order, not completion order: deterministic on replay.
        let had_answer = fan.had_answer;
        fan.reports.sort_by_key(|(idx, _)| *idx);
        fan.failures.sort_by_key(|(idx, _, _)| *idx);
        let engines_used: Vec<EngineReport> = fan.reports.into_iter().map(|(_, r)| r).collect();
        let failures: Vec<(EngineId, EngineError)> =
            fan.failures.into_iter().map(|(_, id, e)| (id, e)).collect();

        // `cauce_deadline_hit_total` counts flights the hard deadline cut,
        // once per flight — including the all-engines-timed-out case that
        // surfaces as `AllEnginesFailed` and never reaches the Ok metrics
        // in `shared_response`.
        if fan.deadline_hit {
            self.metrics.record_deadline_hit();
        }

        // Persist health: breaker transitions flush urgently, routine
        // EWMA/failure updates are debounced to 1/s (settled input).
        if let Err(e) = self.health.flush_due().await {
            warn!(error = %e, "engine health persist failed");
        }

        if !had_answer {
            warn!(failures = failures.len(), "all engines failed");
            // No log write here: every waiter on the flight logs its own
            // failure row in `shared_response`.
            return Err(PipelineError::AllEnginesFailed(failures));
        }

        let resp = SearchResponse {
            query: normalize_query(&ctx.req.q),
            results: merged,
            meta: SearchMeta {
                source: Source::Network,
                engines_used,
                engines_skipped: skipped,
                deadline_hit: fan.deadline_hit,
                hedged: fan.hedged,
                hedge_at_ms: fan.hedge_at_ms,
                elapsed_ms: millis(ctx.started.elapsed()),
                request_id: ctx.request_id,
            },
        };

        // ---- persist -------------------------------------------------------
        let persist = info_span!(
            "persist",
            request_id = %ctx.request_id,
            key = %ctx.key,
            ttl_s = ctx.ttl.as_secs(),
        );
        // Instrument the awaited future; an `Entered` guard held across
        // `.await` would leak the span onto unrelated tasks under a
        // multi-threaded runtime.
        let put = self
            .store
            .put(ctx.key, &resp, ctx.ttl)
            .instrument(persist.clone())
            .await;
        persist.in_scope(|| match put {
            Ok(()) => debug!("response cached"),
            Err(e) => warn!(error = %e, "cache write failed; serving response anyway"),
        });
        Ok(resp)
    }

    /// The unconditional `search_log` write (section 5): called on every
    /// path: cache hit, network, empty and failure. A write failure is
    /// logged and swallowed: a logging outage must not break search.
    pub(super) async fn write_log(
        &self,
        req: &SearchRequest,
        key: &CacheKey,
        query: &str,
        row: LogRow,
        started: Instant,
    ) {
        let row = SearchLogRow {
            id: None,
            ts: Utc::now(),
            query_hash: key.clone(),
            query: query.to_string(),
            // `query` stays normalized for stats/filtering; `query_raw`
            // keeps the user's original casing for history displays (#89).
            query_raw: Some(req.q.clone()),
            client: req.client.clone(),
            source: row.source,
            tier: row.tier,
            latency_ms: millis(started.elapsed()),
            result_count: row.result_count,
            engines: row.engines,
            deadline_hit: row.deadline_hit,
        };
        if let Err(e) = self.store.log_search(row).await {
            warn!(error = %e, "search_log write failed");
        }
    }
}
