//! Tier-1/tier-2 cache lookup, stale overflow serve and background refresh.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{Instrument, debug, info, info_span, warn};
use uuid::Uuid;

use crate::cache::{CacheKey, CachedSearch, lexical_tokens, token_jaccard};
use crate::engine::{Engine, EngineId, Tier};
use crate::health::Gate;
use crate::normalize::normalize_url;
use crate::request::SearchRequest;
use crate::response::{
    EngineStatus, SearchMeta, SearchResponse, SearchResult, Source, StreamEvent, StreamMeta,
};

use super::*;

impl SearchPipeline {
    /// Emit a complete response as one `results` batch per producing
    /// engine plus the terminal `meta` — used by every non-incremental
    /// serve (tier-1/2 hits, stale overflow, singleflight followers). Batch
    /// order is `engines_used` fan-out order; a leftover batch labelled
    /// `merged` carries results whose producing engine is not in the
    /// reports (belt-and-braces for stored payloads).
    pub(super) fn emit_response(
        &self,
        tx: &mpsc::UnboundedSender<StreamEvent>,
        resp: &SearchResponse,
        started: Instant,
    ) {
        let elapsed_ms = millis(started.elapsed());
        let mut emitted = vec![false; resp.results.len()];
        for report in &resp.meta.engines_used {
            if !matches!(report.status, EngineStatus::Ok) {
                continue;
            }
            let mut batch = Vec::new();
            for (i, r) in resp.results.iter().enumerate() {
                if r.engine == report.engine {
                    emitted[i] = true;
                    batch.push(r.clone());
                }
            }
            if !batch.is_empty() {
                let _ = tx.send(StreamEvent::Results {
                    engine: report.engine.clone(),
                    results: batch,
                    elapsed_ms,
                });
            }
        }
        let rest: Vec<SearchResult> = resp
            .results
            .iter()
            .zip(&emitted)
            .filter(|(_, seen)| !**seen)
            .map(|(r, _)| r.clone())
            .collect();
        if !rest.is_empty() {
            let _ = tx.send(StreamEvent::Results {
                engine: EngineId::from("merged"),
                results: rest,
                elapsed_ms,
            });
        }
        let _ = tx.send(StreamEvent::Meta(StreamMeta {
            meta: resp.meta.clone(),
            order: resp.results.iter().map(|r| normalize_url(&r.url)).collect(),
        }));
    }

    /// Enqueue a background refresh for a stale-served key. The refresh
    /// queues for engine permits with a generous budget (nobody is blocked
    /// on it) and only elects itself once it can run immediately, so a
    /// waiting refresh never holds a flight slot that real requests would
    /// join. If another flight for the key registered meanwhile, the
    /// refresh is redundant and exits.
    pub(super) fn spawn_refresh(
        &self,
        req: SearchRequest,
        key: CacheKey,
        runnable: Vec<Arc<dyn Engine>>,
        ttl: Duration,
    ) {
        let pipe = self.clone();
        let span = info_span!("admission.refresh", key = %key, engines = runnable.len());
        tokio::spawn(
            async move {
                let ids: Vec<EngineId> = runnable
                    .iter()
                    .filter(|e| e.tier() != Tier::T2)
                    .map(|e| e.id())
                    .collect();
                // `_permits` must stay bound for the whole fetch; a
                // temporary in the condition would drop the slots before
                // the engine call runs.
                let Ok(_permits) = pipe
                    .admission
                    .acquire_within(&ids, crate::admission::REFRESH_MAX_WAIT)
                    .await
                else {
                    debug!("admission: background refresh dropped, queue stayed full");
                    return;
                };
                let (lead, _rx) = pipe.admission.enter(&key);
                let Some(lead) = lead else {
                    debug!("admission: refresh skipped, another flight is running");
                    return;
                };
                info!("admission: refreshing stale entry");
                let outcome = pipe
                    .fetch(&req, &runnable, &key, ttl, Uuid::now_v7(), Instant::now())
                    .await
                    .map(Arc::new);
                // Requests that joined mid-refresh get the real outcome,
                // error included.
                lead.complete(outcome);
            }
            .instrument(span),
        );
    }

    /// Permit wait exhausted: serve the stored row when one exists. A row
    /// that landed mid-wait (another flight just persisted) is a plain
    /// fresh hit; an expired row goes out `stale` and the caller enqueues
    /// a background refresh; nothing stored means `RateLimited`.
    pub(super) async fn overflow(
        &self,
        key: &CacheKey,
        request_id: Uuid,
        started: Instant,
    ) -> Result<Arc<SearchResponse>, PipelineError> {
        match self.store.get_cache(key).await {
            Ok(Some(row)) => {
                let stale = row.expires_at <= Utc::now();
                if stale {
                    info!(key = %key, stale_served = true, "admission: serving stale row on overflow");
                } else {
                    debug!(key = %key, "admission: overflow resolved by a fresh row");
                }
                Ok(Arc::new(self.cache_hit_response(
                    row,
                    Tier::T1,
                    request_id,
                    started,
                    stale,
                )))
            }
            Ok(None) => Err(self.rate_limited()),
            Err(e) => {
                warn!(key = %key, error = %e, "admission: stale lookup failed");
                Err(self.rate_limited())
            }
        }
    }

    /// Tier-1 `get_cache` for the W3-02 stale-while-revalidate arm: a row
    /// past `expires_at` but inside `self.cache.stale_grace` is servable
    /// stale (the caller enqueues the deduped refresh). A fresh row never
    /// reaches here — `cache_lookup` already served it — and a store
    /// failure degrades to a miss, same as tier 1.
    pub(super) async fn stale_lookup(
        &self,
        key: &CacheKey,
        request_id: Uuid,
    ) -> Option<CachedSearch> {
        let span = info_span!(
            "cache_lookup",
            request_id = %request_id,
            tier = 1u8,
            stale = true,
            hit = tracing::field::Empty,
        );
        // Instrument the awaited future; an `Entered` guard held across
        // `.await` would leak the span onto unrelated tasks under a
        // multi-threaded runtime.
        let result = self.store.get_cache(key).instrument(span.clone()).await;
        span.in_scope(|| match result {
            Ok(Some(row)) => {
                let now = Utc::now();
                let grace = chrono::Duration::from_std(self.cache.stale_grace)
                    .unwrap_or(chrono::Duration::MAX);
                let servable = row.expires_at <= now && now <= row.expires_at + grace;
                span.record("hit", servable);
                if servable {
                    debug!(key = %key, "stale row inside the grace window");
                    Some(row)
                } else {
                    debug!(key = %key, "no stale row inside the grace window");
                    None
                }
            }
            Ok(None) => {
                span.record("hit", false);
                None
            }
            Err(e) => {
                span.record("hit", false);
                warn!(key = %key, error = %e, "stale lookup failed; treating as miss");
                None
            }
        })
    }

    /// The `cauce_stale_served_total{reason}` label for a stale serve:
    /// the caller's serve-path reason, unless every pinned engine is
    /// unhealthy — a stale serve in an all-breaker outage is the outage
    /// signal (W3-02 rule c) and is labelled `engines_unhealthy` plus a
    /// warn event. The health read is `peek`, not `admission`: this check
    /// must not claim a half-open probe slot a fan-out could have used.
    pub(super) fn stale_reason(
        &self,
        runnable: &[Arc<dyn Engine>],
        reason: &'static str,
    ) -> &'static str {
        if !runnable.is_empty()
            && runnable
                .iter()
                .all(|e| matches!(self.health.peek(&e.id()), Gate::Skip))
        {
            warn!(
                engines = %render_ids(&runnable.iter().map(|e| e.id()).collect::<Vec<_>>()),
                "serving stale row: every pinned engine is unhealthy"
            );
            "engines_unhealthy"
        } else {
            reason
        }
    }

    /// Tier-1 `get_exact` under a `cache_lookup` span. A store failure is
    /// degraded to a miss (the network path still serves the request).
    pub(super) async fn cache_lookup(
        &self,
        key: &CacheKey,
        request_id: Uuid,
    ) -> Option<CachedSearch> {
        let span = info_span!(
            "cache_lookup",
            request_id = %request_id,
            tier = 1u8,
            hit = tracing::field::Empty,
            age_s = tracing::field::Empty,
        );
        // Instrument the awaited future; an `Entered` guard held across
        // `.await` would leak the span onto unrelated tasks under a
        // multi-threaded runtime.
        let result = self.store.get_exact(key).instrument(span.clone()).await;
        span.in_scope(|| match result {
            Ok(hit) => {
                span.record("hit", hit.is_some());
                if let Some(entry) = &hit {
                    let age_s = Utc::now()
                        .signed_duration_since(entry.created_at)
                        .num_seconds()
                        .max(0) as u64;
                    span.record("age_s", age_s);
                    debug!(age_s, "cache hit");
                } else {
                    debug!("cache miss");
                }
                hit
            }
            Err(e) => {
                span.record("hit", false);
                warn!(error = %e, "cache lookup failed; treating as miss");
                None
            }
        })
    }

    /// Tier-2 `get_lexical` under a `cache_lookup` span (W1-10).
    ///
    /// Candidates come back BM25-ranked; they are checked best-first and the
    /// first one passing the gate is served. The gate: the row is fresh
    /// (expired rows are skipped — serving stale belongs to admission,
    /// W1-07), the stored `key` equals the key this request's params would
    /// produce for the candidate's query — that is the "same page/lang"
    /// requirement, checked via the key preimage because `params_json` does
    /// not record page/lang; it also pins time_range/safesearch/engines,
    /// which only ever rejects more, never wrongfully accepts — and the
    /// Jaccard similarity of the stopword-free token sets clears
    /// `lexical.threshold`.
    ///
    /// A store failure degrades to a miss, same as tier 1.
    pub(super) async fn lexical_lookup(
        &self,
        req: &SearchRequest,
        query: &str,
        request_id: Uuid,
    ) -> Option<CachedSearch> {
        let span = info_span!(
            "cache_lookup",
            request_id = %request_id,
            tier = 2u8,
            hit = tracing::field::Empty,
            candidates = tracing::field::Empty,
            age_s = tracing::field::Empty,
            matched = tracing::field::Empty,
        );
        let want = lexical_tokens(query);
        if want.is_empty() {
            span.in_scope(|| {
                span.record("hit", false);
                span.record("candidates", 0u64);
            });
            debug!("query has no lexical tokens; skipping tier-2 lookup");
            return None;
        }
        let result = self
            .store
            .get_lexical(query, 5)
            .instrument(span.clone())
            .await;
        span.in_scope(|| {
            let rows = match result {
                Ok(rows) => rows,
                Err(e) => {
                    span.record("hit", false);
                    warn!(error = %e, "lexical lookup failed; treating as miss");
                    return None;
                }
            };
            span.record("candidates", rows.len() as u64);
            let now = Utc::now();
            for cand in rows {
                if cand.expires_at <= now {
                    continue;
                }
                // Same page/lang (and the rest of the key preimage): rebuild
                // the key this request would produce for the candidate's
                // query and compare to the stored key.
                let mut shadow = req.clone();
                shadow.q.clone_from(&cand.query);
                if CacheKey::from(&shadow) != cand.key {
                    continue;
                }
                let score = token_jaccard(&want, &lexical_tokens(&cand.query));
                if score < self.lexical.threshold {
                    continue;
                }
                let age_s = now
                    .signed_duration_since(cand.created_at)
                    .num_seconds()
                    .max(0) as u64;
                span.record("hit", true);
                span.record("age_s", age_s);
                span.record("matched", cand.query.as_str());
                debug!(matched = %cand.query, score, "tier-2 cache hit");
                return Some(cand);
            }
            span.record("hit", false);
            debug!("lexical candidates rejected by the gate");
            None
        })
    }

    /// Rebuild a stored row as a cache response: provenance
    /// (`engines_used`, `query`) is kept while `source`, `elapsed_ms` and
    /// `request_id` describe this request. `ttl_s` is the remaining TTL
    /// (0 on an expired row); `stale` marks rows past `expires_at` served
    /// by the admission-overflow path (W1-07). Fuzzy tiers (2+) carry
    /// `matched_query` (the stored query); an exact tier-1 hit leaves it
    /// `None`.
    pub(super) fn cache_hit_response(
        &self,
        hit: CachedSearch,
        tier: Tier,
        request_id: Uuid,
        started: Instant,
        stale: bool,
    ) -> SearchResponse {
        let now = Utc::now();
        let age_s = now
            .signed_duration_since(hit.created_at)
            .num_seconds()
            .max(0) as u64;
        let ttl_s = hit
            .expires_at
            .signed_duration_since(now)
            .num_seconds()
            .max(0) as u64;
        let matched_query = (tier != Tier::T1).then(|| hit.query.clone());
        let mut resp = hit.response;
        resp.meta = SearchMeta {
            source: Source::Cache {
                tier,
                age_s,
                ttl_s,
                stale,
                matched_query,
            },
            engines_used: resp.meta.engines_used,
            engines_skipped: Vec::new(),
            deadline_hit: false,
            // Per-request fields like `engines_skipped`: a cached row
            // answers without a fan-out, so it was never hedged.
            hedged: false,
            hedge_at_ms: None,
            elapsed_ms: millis(started.elapsed()),
            request_id,
        };
        resp
    }
}
