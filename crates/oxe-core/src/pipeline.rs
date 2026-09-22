//! `SearchPipeline` v0 (W0-08, parent plan section 4.4 minus hedging,
//! breakers and tier-2/3 lookups, which land in W1/W3).
//!
//! Request flow for [`SearchPipeline::search`] /
//! [`SearchPipeline::search_opts`]:
//!
//! 1. Normalise the query ([`normalize_query`]; `CacheKey` normalises again
//!    internally for hashing) and compute the [`CacheKey`].
//! 2. Tier-1 `store.get_exact`: a fresh hit is served with
//!    `meta.source = Source::Cache { tier: 1, age_s, ttl_s: remaining,
//!    stale: false }` and *still* appends a `search_log` row
//!    (`LogSource::Cache`): the unconditional write is the whole point of
//!    the design (v2 skipped it and its hit-rate stats were fiction).
//! 3. On a miss, fan out to every configured engine in parallel
//!    ([`tokio::task::JoinSet`]), each call wrapped in
//!    `tokio::time::timeout(deadline)`. Engines cut off at the deadline
//!    report `EngineStatus::Failed(EngineError::Timeout)` and set
//!    `meta.deadline_hit`. `req.engines = Some(ids)` pins the fan-out to
//!    the configured engines whose ids are in the set.
//! 4. Merge: dedupe by [`normalize_url`], RRF `k = 60` summed across
//!    engines (`score = sum 1/(60 + rank)`, rank 1-based per engine),
//!    stable ordering by first-seen position; the occurrence with the
//!    best single contribution supplies the emitted `SearchResult`.
//! 5. `store.put` with `ttl = opts.ttl.unwrap_or(default_ttl)` clamped to
//!    `ttl_cap` (defaults 3600 s / 86400 s). Never on the
//!    `AllEnginesFailed` path.
//! 6. `store.log_search` unconditionally: cache hit, network, empty and
//!    error paths all write a row.
//!
//! Request ids: `search` mints a UUIDv7; `search_with_id`/`search_opts`
//! take a caller-supplied `Uuid` so `meta.request_id` == `X-Request-Id` ==
//! the JSONL `request_id` field. W0-09 middleware passes
//! `RequestId::as_uuid()` here.
//!
//! Tracing: `pipeline.search` is the root span (`request_id`, `query`,
//! `page`, `client`, `engines` = runnable count once the pin is applied).
//! Children: `cache_lookup` (`tier`, `hit`, `age_s`), one `engine` span
//! per fanned-out engine (`engine`, `tier`,
//! `status`, `results`; wall duration lands in `busy_ms` on close),
//! `merge` (`in`, `out`, `deadline_hit`), `persist` (`key`, `ttl_s`).
//! The JSONL layer hoists `request_id` from the enclosing scope, so
//! `oxe trace <id>` rebuilds the whole fan-out.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tracing::{Instrument, debug, info, info_span, warn};
use url::Url;
use uuid::Uuid;

use crate::cache::{CacheKey, CachedSearch, normalize_query};
use crate::engine::{Engine, EngineError, EngineId, Tier};
use crate::normalize::normalize_url;
use crate::request::SearchRequest;
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source,
};
use crate::store::{LogSource, SearchLogRow, Store};

/// Hard fan-out deadline when the caller does not configure one
/// (`search.deadline_ms` in config once W0-11 lands).
pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(3_000);

/// Cache TTL for stored responses when no per-call override is passed.
pub const DEFAULT_TTL: Duration = Duration::from_secs(3_600);

/// Ceiling applied to every TTL, default or per-call override (24 h).
pub const DEFAULT_TTL_CAP: Duration = Duration::from_secs(86_400);

/// RRF constant (parent plan 4.4 step 5).
const RRF_K: f32 = 60.0;

/// Per-call options for [`SearchPipeline::search_opts`].
///
/// `SearchRequest` deliberately carries no `ttl_s` field (settled
/// contract), so the per-call TTL override lives here; a config-level
/// `cache.ttl_s` can instead be baked into [`SearchPipeline::with_default_ttl`]
/// at construction.
#[derive(Debug, Clone, Default)]
pub struct SearchOpts {
    /// Request id stamped on `meta.request_id` and every span's
    /// `request_id` field. `None` mints a fresh UUIDv7.
    pub request_id: Option<Uuid>,
    /// Per-call cache TTL override, clamped to `ttl_cap`. `None` uses the
    /// pipeline's `default_ttl`.
    pub ttl: Option<Duration>,
}

/// Errors a search can return.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// The engine pin in `req.engines` matched no configured engine, or
    /// the pipeline was built with an empty engine list.
    ///
    /// W0-09 maps the two cases differently: a bad pin is a 400, an
    /// unconfigured pipeline a 503. Call [`SearchPipeline::configured_engines`]
    /// to distinguish (`0` means no engines configured).
    #[error("no engines available for this request")]
    NoEngines,
    /// Every engine that ran returned an error. The response is not
    /// cached; the `search_log` row is still written.
    #[error("all engines failed: {}", render_failures(.0))]
    AllEnginesFailed(Vec<(EngineId, EngineError)>),
}

impl PipelineError {
    /// The per-engine failures of `AllEnginesFailed`; empty otherwise.
    pub fn failures(&self) -> &[(EngineId, EngineError)] {
        match self {
            Self::AllEnginesFailed(failures) => failures,
            Self::NoEngines => &[],
        }
    }
}

fn render_failures(failures: &[(EngineId, EngineError)]) -> String {
    failures
        .iter()
        .map(|(id, e)| format!("{id}: {e}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// One completed engine call. `idx` is the position in the runnable list
/// so duplicate engine ids (two `replay` instances) stay distinguishable.
struct EngineOutcome {
    idx: usize,
    id: EngineId,
    latency: Duration,
    /// `Err(Elapsed)` means the hard deadline fired, not the engine.
    outcome: Result<Result<Vec<SearchResult>, EngineError>, tokio::time::error::Elapsed>,
}

/// The search pipeline: tier-1 cache lookup, parallel fan-out under a
/// hard deadline, RRF merge, persist, unconditional `search_log`.
///
/// Build once at startup and share by reference; all fields are immutable
/// after construction. `search*` must be called inside a tokio runtime;
/// `JoinSet::spawn` panics outside one.
pub struct SearchPipeline {
    store: Arc<dyn Store>,
    engines: Vec<Arc<dyn Engine>>,
    deadline: Duration,
    default_ttl: Duration,
    ttl_cap: Duration,
}

fn millis(d: Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

impl SearchPipeline {
    /// A pipeline with the documented defaults (deadline 3000 ms, TTL
    /// 3600 s capped at 86400 s).
    pub fn new(store: Arc<dyn Store>, engines: Vec<Arc<dyn Engine>>) -> Self {
        Self {
            store,
            engines,
            deadline: DEFAULT_DEADLINE,
            default_ttl: DEFAULT_TTL,
            ttl_cap: DEFAULT_TTL_CAP,
        }
    }

    /// Override the hard fan-out deadline (`search.deadline_ms`).
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Override the cache TTL applied when no per-call override is passed.
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Override the ceiling applied to every cache TTL.
    pub fn with_ttl_cap(mut self, cap: Duration) -> Self {
        self.ttl_cap = cap;
        self
    }

    /// Number of configured engines. `0` means the pipeline is
    /// unconfigured: W0-09 maps `NoEngines` + `0` to 503 and
    /// `NoEngines` + `> 0` (pin matched nothing) to 400.
    pub fn configured_engines(&self) -> usize {
        self.engines.len()
    }

    /// Search with a fresh UUIDv7 request id.
    pub async fn search(&self, req: &SearchRequest) -> Result<SearchResponse, PipelineError> {
        self.search_opts(req, SearchOpts::default()).await
    }

    /// Search under a caller-supplied request id (W0-09 middleware passes
    /// `RequestId::as_uuid()`; W0-12 CLI mints one per invocation).
    pub async fn search_with_id(
        &self,
        req: &SearchRequest,
        request_id: Uuid,
    ) -> Result<SearchResponse, PipelineError> {
        self.search_opts(
            req,
            SearchOpts {
                request_id: Some(request_id),
                ttl: None,
            },
        )
        .await
    }

    /// Search with explicit per-call options (`request_id`, `ttl`).
    pub async fn search_opts(
        &self,
        req: &SearchRequest,
        opts: SearchOpts,
    ) -> Result<SearchResponse, PipelineError> {
        let request_id = opts.request_id.unwrap_or_else(Uuid::now_v7);
        let span = info_span!(
            "pipeline.search",
            request_id = %request_id,
            query = %normalize_query(&req.q),
            page = req.page,
            client = %req.client,
            // Runnable count (post-pin) is recorded in `run` once known.
            engines = tracing::field::Empty,
        );
        self.run(req, opts.ttl, request_id).instrument(span).await
    }

    async fn run(
        &self,
        req: &SearchRequest,
        ttl_override: Option<Duration>,
        request_id: Uuid,
    ) -> Result<SearchResponse, PipelineError> {
        let started = Instant::now();
        let query = normalize_query(&req.q);
        let key = CacheKey::from(req);

        // ---- tier-1 exact lookup ---------------------------------------
        if let Some(hit) = self.cache_lookup(&key, request_id).await {
            // The `engines` column records the engines whose results are
            // being served (`cache_entries.engines_json` provenance).
            let engines = hit.engines.clone();
            let resp = self.cache_hit_response(hit, request_id, started);
            self.write_log(
                req,
                &key,
                &query,
                LogRow {
                    source: LogSource::Cache,
                    tier: Some(Tier::T1),
                    result_count: resp.results.len() as u32,
                    engines,
                    deadline_hit: false,
                },
                started,
            )
            .await;
            info!(
                source = "cache",
                results = resp.results.len(),
                elapsed_ms = resp.meta.elapsed_ms,
                "search complete"
            );
            return Ok(resp);
        }

        // ---- fan-out ----------------------------------------------------
        let runnable = self.runnable(req);
        // `pipeline.search` is the current span here (via `instrument`).
        tracing::Span::current().record("engines", runnable.len() as u64);
        if runnable.is_empty() {
            warn!(pinned = ?req.engines, "no engines to run");
            self.write_log(
                req,
                &key,
                &query,
                LogRow {
                    source: LogSource::Network,
                    tier: None,
                    result_count: 0,
                    engines: Vec::new(),
                    deadline_hit: false,
                },
                started,
            )
            .await;
            return Err(PipelineError::NoEngines);
        }
        let outcomes = self.fan_out(req, &runnable, request_id).await;

        // ---- per-engine reports ------------------------------------------
        let mut reports: Vec<(usize, EngineReport)> = Vec::with_capacity(runnable.len());
        let mut ok_results: Vec<(usize, Vec<SearchResult>)> = Vec::new();
        let mut failures: Vec<(usize, EngineId, EngineError)> = Vec::new();
        let mut deadline_hit = false;
        let mut answered = vec![false; runnable.len()];

        for outcome in outcomes {
            let idx = outcome.idx;
            answered[idx] = true;
            let latency_ms = millis(outcome.latency);
            match outcome.outcome {
                Ok(Ok(results)) => {
                    reports.push((
                        idx,
                        EngineReport {
                            engine: outcome.id.clone(),
                            status: EngineStatus::Ok,
                            latency_ms,
                            result_count: results.len() as u32,
                        },
                    ));
                    ok_results.push((idx, results));
                }
                // `NoResults` is an answer, not a failure: the engine
                // responded and there is simply no page to serve (the exec
                // protocol's first-class code; `replay` uses it for pages
                // beyond `page_limit`). It counts toward "an engine
                // answered" so an all-`NoResults` fan-out is a 200-shaped
                // empty response, not `AllEnginesFailed` (v2's "page 2
                // always 502" defect). The report stays `Failed(NoResults)`
                // for honesty.
                Ok(Err(EngineError::NoResults)) => {
                    reports.push((
                        idx,
                        EngineReport {
                            engine: outcome.id.clone(),
                            status: EngineStatus::Failed(EngineError::NoResults),
                            latency_ms,
                            result_count: 0,
                        },
                    ));
                    ok_results.push((idx, Vec::new()));
                }
                Ok(Err(err)) => {
                    reports.push((
                        idx,
                        EngineReport {
                            engine: outcome.id.clone(),
                            status: EngineStatus::Failed(err.clone()),
                            latency_ms,
                            result_count: 0,
                        },
                    ));
                    failures.push((idx, outcome.id, err));
                }
                Err(_elapsed) => {
                    deadline_hit = true;
                    reports.push((
                        idx,
                        EngineReport {
                            engine: outcome.id.clone(),
                            status: EngineStatus::Failed(EngineError::Timeout),
                            latency_ms,
                            result_count: 0,
                        },
                    ));
                    failures.push((idx, outcome.id, EngineError::Timeout));
                }
            }
        }
        // A JoinError (panic/cancel) never names its engine; the unanswered
        // slots are exactly those failures.
        for (idx, engine) in runnable.iter().enumerate() {
            if answered[idx] {
                continue;
            }
            let id = engine.id();
            reports.push((
                idx,
                EngineReport {
                    engine: id.clone(),
                    status: EngineStatus::Failed(EngineError::Transport(
                        "engine task failed".to_string(),
                    )),
                    latency_ms: millis(started.elapsed()),
                    result_count: 0,
                },
            ));
            failures.push((
                idx,
                id,
                EngineError::Transport("engine task failed".to_string()),
            ));
        }
        // Fan-out order, not completion order: deterministic on replay.
        reports.sort_by_key(|(idx, _)| *idx);
        failures.sort_by_key(|(idx, _, _)| *idx);
        let engines_used: Vec<EngineReport> = reports.into_iter().map(|(_, r)| r).collect();
        let failures: Vec<(EngineId, EngineError)> =
            failures.into_iter().map(|(_, id, e)| (id, e)).collect();

        if ok_results.is_empty() {
            warn!(failures = failures.len(), "all engines failed");
            self.write_log(
                req,
                &key,
                &query,
                LogRow {
                    source: LogSource::Network,
                    tier: None,
                    result_count: 0,
                    engines: runnable.iter().map(|e| e.id()).collect(),
                    deadline_hit,
                },
                started,
            )
            .await;
            return Err(PipelineError::AllEnginesFailed(failures));
        }

        // ---- merge: dedupe by normalized URL, RRF k=60 ---------------------
        ok_results.sort_by_key(|(idx, _)| *idx);
        let merged = {
            let span = info_span!(
                "merge",
                request_id = %request_id,
                r#in = tracing::field::Empty,
                out = tracing::field::Empty,
                deadline_hit,
            );
            let _e = span.enter();
            let raw: usize = ok_results.iter().map(|(_, r)| r.len()).sum();
            let merged = merge_rrf(ok_results.iter().map(|(_, results)| results.as_slice()));
            span.record("in", raw as u64);
            span.record("out", merged.len() as u64);
            debug!(raw, merged = merged.len(), deadline_hit, "merged results");
            merged
        };

        let resp = SearchResponse {
            query: query.clone(),
            results: merged,
            meta: SearchMeta {
                source: Source::Network,
                engines_used,
                deadline_hit,
                elapsed_ms: millis(started.elapsed()),
                request_id,
            },
        };

        // ---- persist + unconditional log -----------------------------------
        let ttl = ttl_override.unwrap_or(self.default_ttl).min(self.ttl_cap);
        let persist = info_span!(
            "persist",
            request_id = %request_id,
            key = %key,
            ttl_s = ttl.as_secs(),
        );
        // Instrument the awaited future; an `Entered` guard held across
        // `.await` would leak the span onto unrelated tasks under a
        // multi-threaded runtime.
        let put = self
            .store
            .put(&key, &resp, ttl)
            .instrument(persist.clone())
            .await;
        persist.in_scope(|| match put {
            Ok(()) => debug!("response cached"),
            Err(e) => warn!(error = %e, "cache write failed; serving response anyway"),
        });
        self.write_log(
            req,
            &key,
            &query,
            LogRow {
                source: LogSource::Network,
                tier: None,
                result_count: resp.results.len() as u32,
                engines: runnable.iter().map(|e| e.id()).collect(),
                deadline_hit,
            },
            started,
        )
        .await;
        info!(
            source = "network",
            results = resp.results.len(),
            elapsed_ms = resp.meta.elapsed_ms,
            deadline_hit,
            "search complete"
        );
        Ok(resp)
    }

    /// Engines that run for `req`: the configured list, or the subset a
    /// `req.engines` pin selects.
    fn runnable(&self, req: &SearchRequest) -> Vec<Arc<dyn Engine>> {
        match &req.engines {
            Some(ids) => self
                .engines
                .iter()
                .filter(|e| ids.contains(&e.id()))
                .cloned()
                .collect(),
            None => self.engines.clone(),
        }
    }

    /// Tier-1 `get_exact` under a `cache_lookup` span. A store failure is
    /// degraded to a miss (the network path still serves the request).
    async fn cache_lookup(&self, key: &CacheKey, request_id: Uuid) -> Option<CachedSearch> {
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

    /// Parallel fan-out with a hard deadline per engine. Returns one
    /// [`EngineOutcome`] per task that answered or timed out; panicking
    /// tasks are logged and reconciled by the caller via `answered`.
    async fn fan_out(
        &self,
        req: &SearchRequest,
        runnable: &[Arc<dyn Engine>],
        request_id: Uuid,
    ) -> Vec<EngineOutcome> {
        let mut set = tokio::task::JoinSet::new();
        for (idx, engine) in runnable.iter().enumerate() {
            let engine = engine.clone();
            let id = engine.id();
            let deadline = self.deadline;
            // Created while `pipeline.search` is the current span, so this
            // span's parent is the search span; `request_id` is also
            // recorded directly so the field survives even if the span is
            // ever emitted detached.
            let span = info_span!(
                "engine",
                request_id = %request_id,
                engine = %id,
                tier = engine.tier().as_u8(),
                status = tracing::field::Empty,
                results = tracing::field::Empty,
            );
            let req2 = req.clone();
            // A second handle on the same span for post-hoc `record`s;
            // `instrument` consumes the other.
            let recorder = span.clone();
            set.spawn(
                async move {
                    let t0 = Instant::now();
                    let outcome =
                        tokio::time::timeout(deadline, engine.search(&req2, deadline)).await;
                    let latency = t0.elapsed();
                    match &outcome {
                        Ok(Ok(r)) => {
                            recorder.record("status", "ok");
                            recorder.record("results", r.len() as u64);
                            debug!(results = r.len(), "engine done");
                        }
                        Ok(Err(e)) => {
                            recorder.record("status", "error");
                            recorder.record("results", 0u64);
                            debug!(error = %e, "engine failed");
                        }
                        Err(_) => {
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
        let mut outcomes = Vec::with_capacity(runnable.len());
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(outcome) => outcomes.push(outcome),
                Err(join_err) => {
                    warn!(error = %join_err, "engine task failed to join");
                }
            }
        }
        outcomes
    }

    /// Rebuild the stored payload as a fresh-hit response: provenance
    /// (`engines_used`, `query`) is kept while `source`, `elapsed_ms` and
    /// `request_id` describe this request. `ttl_s` is the remaining TTL.
    fn cache_hit_response(
        &self,
        hit: CachedSearch,
        request_id: Uuid,
        started: Instant,
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
        let mut resp = hit.response;
        resp.meta = SearchMeta {
            source: Source::Cache {
                tier: Tier::T1,
                age_s,
                ttl_s,
                stale: false,
            },
            engines_used: resp.meta.engines_used,
            deadline_hit: false,
            elapsed_ms: millis(started.elapsed()),
            request_id,
        };
        resp
    }

    /// The unconditional `search_log` write (section 5): called on every
    /// path: cache hit, network, empty and failure. A write failure is
    /// logged and swallowed: a logging outage must not break search.
    async fn write_log(
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

/// Fields of a `SearchLogRow` the call site supplies; `write_log` fills
/// `id`, `ts`, `query_hash`, `query`, `client` and `latency_ms`.
struct LogRow {
    source: LogSource,
    tier: Option<Tier>,
    result_count: u32,
    engines: Vec<EngineId>,
    deadline_hit: bool,
}

/// RRF merge over per-engine result lists in engine order: each occurrence
/// contributes `1/(60 + rank)` (rank 1-based within its own list),
/// duplicates are keyed by [`normalize_url`], and the occurrence with the
/// best single contribution supplies the emitted `SearchResult`. Output is
/// ordered by descending score, stable on first-seen position for ties.
fn merge_rrf<'a>(lists: impl Iterator<Item = &'a [SearchResult]>) -> Vec<SearchResult> {
    struct Acc {
        result: SearchResult,
        score: f32,
        best: f32,
    }
    let mut order: Vec<Url> = Vec::new();
    let mut map: HashMap<Url, Acc> = HashMap::new();
    for results in lists {
        for (rank0, r) in results.iter().enumerate() {
            let contrib = 1.0 / (RRF_K + rank0 as f32 + 1.0);
            // Dedupe on the normalized form but emit the engine's raw URL:
            // SearXNG-parity behaviour, the displayed link is untouched.
            let norm = normalize_url(&r.url);
            match map.entry(norm.clone()) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(Acc {
                        result: r.clone(),
                        score: contrib,
                        best: contrib,
                    });
                    order.push(norm);
                }
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    let acc = o.get_mut();
                    acc.score += contrib;
                    if contrib > acc.best {
                        acc.best = contrib;
                        acc.result = r.clone();
                    }
                }
            }
        }
    }
    let mut merged: Vec<SearchResult> = order
        .into_iter()
        .filter_map(|u| map.remove(&u))
        .map(|acc| SearchResult {
            score: acc.score,
            ..acc.result
        })
        .collect();
    // `sort_by` is stable: ties keep first-seen (engine order, then rank).
    merged.sort_by(|a, b| b.score.total_cmp(&a.score));
    merged
}
