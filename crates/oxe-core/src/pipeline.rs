//! `SearchPipeline` v0 (W0-08, parent plan section 4.4 minus hedging,
//! breakers and the tier-3 lookup, which land in W1/W3) plus W1-07
//! admission control and the W1-10 tier-2 lexical lookup.
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
//! 3. On a miss, admission (see [`crate::admission`]): singleflight on
//!    `CacheKey` elects one leader per key and followers await the shared
//!    outcome. The leader's detached flight task first runs the tier-2
//!    `store.get_lexical(q, 5)` lookup (W1-10, `cache.lexical.*`):
//!    candidates are checked best-rank first; a row is accepted when it
//!    is fresh, its stored `key` equals the key this request's params
//!    would produce for the candidate's query (the "same page/lang" gate
//!    — the preimage also pins time_range/safesearch/engines, which is
//!    strictly conservative), and its query's token set clears the
//!    Jaccard `threshold` (default 0.8) after normalisation and stopword
//!    removal. A hit is served as
//!    `Source::Cache { tier: 2, matched_query: Some(..), .. }` without
//!    spending an engine permit. Otherwise the flight waits for
//!    per-engine semaphore permits up to `admission.max_wait_ms`; a
//!    timed-out wait overflows to the stored row if one exists (fresh
//!    rows serve as a normal hit, expired rows go out
//!    `Source::Cache{stale:true}` and a background refresh is enqueued)
//!    else fails with [`PipelineError::RateLimited`].
//! 4. The flight fans out to every configured engine in parallel
//!    ([`tokio::task::JoinSet`]), each call wrapped in
//!    `tokio::time::timeout(deadline)`. Engines cut off at the deadline
//!    report `EngineStatus::Failed(EngineError::Timeout)` and set
//!    `meta.deadline_hit`. `req.engines = Some(ids)` pins the fan-out to
//!    the configured engines whose ids are in the set.
//! 5. Merge: dedupe by [`normalize_url`], RRF `k = 60` summed across
//!    engines (`score = sum 1/(60 + rank)`, rank 1-based per engine),
//!    stable ordering by first-seen position; the occurrence with the
//!    best single contribution supplies the emitted `SearchResult`.
//! 6. `store.put` with `ttl = opts.ttl.unwrap_or(default_ttl)` clamped to
//!    `ttl_cap` (defaults 3600 s / 86400 s), once per flight. Never on the
//!    `AllEnginesFailed`/`RateLimited` paths.
//! 7. `store.log_search` unconditionally: every request — cache hit,
//!    singleflight follower, stale serve, error — writes its own row with
//!    its own `request_id`; the shared `SearchResponse`'s
//!    `meta.request_id`/`elapsed_ms` are rewritten per waiter so the
//!    `meta.request_id == X-Request-Id == JSONL request_id` invariant
//!    holds for followers too.
//!
//! Request ids: `search` mints a UUIDv7; `search_with_id`/`search_opts`
//! take a caller-supplied `Uuid` so `meta.request_id` == `X-Request-Id` ==
//! the JSONL `request_id` field. W0-09 middleware passes
//! `RequestId::as_uuid()` here.
//!
//! Tracing: `pipeline.search` is the root span (`request_id`, `query`,
//! `page`, `client`, `engines` = runnable count once the pin is applied).
//! Children: `cache_lookup` (`tier`, `hit`, `age_s`; the tier-2 span also
//! records `candidates` and `matched`), one `engine` span
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

use crate::admission::{Admission, FlightResult, Lead};
use crate::cache::{CacheKey, CachedSearch, lexical_tokens, normalize_query, token_jaccard};
use crate::config::LexicalConfig;
use crate::engine::{Engine, EngineError, EngineId, Tier};
use crate::metrics::{Metrics, engine_error_label};
use crate::normalize::normalize_url;
use crate::request::SearchRequest;
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source,
};
use crate::store::{BreakerState, LogSource, SearchLogRow, Store};

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
///
/// `Clone` because an outcome is published once per admission flight and
/// every waiter on it receives its own copy.
#[derive(Debug, Clone, thiserror::Error)]
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
    /// Admission rejected the request: the per-engine queue wait exceeded
    /// `admission.max_wait_ms` and no stored row (fresh or stale) existed
    /// to serve instead. W0-09 maps this to HTTP 429 with `Retry-After:
    /// retry_after_s`; the MCP surface (W1-08) maps it to a
    /// `rate_limited` tool error carrying `retry_after_s`.
    #[error("admission queue saturated; retry after {retry_after_s}s")]
    RateLimited {
        /// Seconds the client should wait before retrying.
        retry_after_s: u64,
    },
}

impl PipelineError {
    /// The per-engine failures of `AllEnginesFailed`; empty otherwise.
    pub fn failures(&self) -> &[(EngineId, EngineError)] {
        match self {
            Self::AllEnginesFailed(failures) => failures,
            Self::NoEngines | Self::RateLimited { .. } => &[],
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

/// The search pipeline: tier-1 cache lookup, admission (singleflight +
/// bounded per-engine queue), parallel fan-out under a hard deadline,
/// RRF merge, persist, unconditional `search_log`.
///
/// Build once at startup and share by reference; all fields are immutable
/// after construction. `Clone` is shallow (every field is an `Arc` or a
/// value): flight and refresh tasks hold a clone so they outlive the
/// request that elected them. `search*` must be called inside a tokio
/// runtime; `JoinSet::spawn`/`tokio::spawn` panic outside one.
#[derive(Clone)]
pub struct SearchPipeline {
    store: Arc<dyn Store>,
    engines: Vec<Arc<dyn Engine>>,
    deadline: Duration,
    default_ttl: Duration,
    ttl_cap: Duration,
    lexical: LexicalConfig,
    admission: Admission,
    /// W1-09 metrics handle. A unit struct: every `record_*` writes into
    /// the process-global registry, so pipelines share one set of series.
    metrics: Metrics,
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
            lexical: LexicalConfig::default(),
            admission: Admission::default(),
            metrics: Metrics::default(),
        }
    }

    /// Bind a specific `Metrics` handle (`oxe-server` passes the provider
    /// installed for `GET /metrics` + OTLP; tests pass a test-local one).
    pub fn with_metrics(mut self, metrics: Metrics) -> Self {
        self.metrics = metrics;
        self
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

    /// Tier-2 lexical cache settings (`cache.lexical.*`, W1-10). Defaults
    /// to enabled with a 0.8 Jaccard threshold.
    pub fn with_lexical(mut self, lexical: LexicalConfig) -> Self {
        self.lexical = lexical;
        self
    }

    /// Override the admission controller (`admission.max_wait_ms`,
    /// per-engine concurrency cap). Default: [`Admission::default`]
    /// (1500 ms wait, 3 concurrent calls per engine).
    pub fn with_admission(mut self, admission: Admission) -> Self {
        self.admission = admission;
        self
    }

    /// Number of configured engines. `0` means the pipeline is
    /// unconfigured. W0-09 maps `NoEngines` + (zero configured engines or
    /// an empty `engines` pin) to 503 `no_engines`, and `NoEngines` + a
    /// non-empty `engines` pin that matched nothing to 400 `unknown_engines`
    /// regardless of the configured count.
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
            let resp = self.cache_hit_response(hit, Tier::T1, request_id, started, false);
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
            let tier = match resp.meta.source {
                Source::Cache { tier, .. } => Some(tier),
                Source::Network => None,
            };
            self.metrics
                .record_search(&req.client, "cache", tier, started.elapsed());
            return Ok(resp);
        }

        // ---- fan-out set ---------------------------------------------------
        // ---- tier-2 lexical lookup (W1-10) ------------------------------
        if self.lexical.enabled
            && let Some(hit) = self.lexical_lookup(req, &query, request_id).await
        {
            let engines = hit.engines.clone();
            let resp = self.cache_hit_response(hit, Tier::T2, request_id, started, false);
            self.write_log(
                req,
                &key,
                &query,
                LogRow {
                    source: LogSource::Cache,
                    tier: Some(Tier::T2),
                    result_count: resp.results.len() as u32,
                    engines,
                    deadline_hit: false,
                },
                started,
            )
            .await;
            info!(
                source = "cache",
                tier = 2u8,
                results = resp.results.len(),
                elapsed_ms = resp.meta.elapsed_ms,
                "search complete"
            );
            self.metrics
                .record_search(&req.client, "cache", Some(Tier::T2), started.elapsed());
            return Ok(resp);
        }

        // ---- fan-out ----------------------------------------------------
        let runnable = self.runnable(req);
        // Pre-W1-06 there is no breaker bookkeeping: every runnable engine
        // is Closed. W1-06 records real state transitions on this gauge.
        for engine in &runnable {
            self.metrics
                .record_breaker_state(&engine.id(), BreakerState::Closed);
        }
        // `pipeline.search` is the current span here (via `instrument`).
        tracing::Span::current().record("engines", runnable.len() as u64);

        // ---- admission: singleflight + bounded per-engine queue (W1-07) ----
        //
        // Every miss for `key` elects one leader; its detached flight task
        // does the work below and publishes a `FlightResult`. Followers (and
        // the leader's own request) all wait on the same watch channel, so a
        // cancelled request can never strand work its twins are waiting on.
        let ttl = ttl_override.unwrap_or(self.default_ttl).min(self.ttl_cap);
        let mut re_elected = false;
        let shared: FlightResult = loop {
            let (lead, mut rx) = self.admission.enter(&key);
            if let Some(lead) = lead {
                self.spawn_flight(
                    lead,
                    req.clone(),
                    key.clone(),
                    runnable.clone(),
                    ttl,
                    request_id,
                );
            }
            // Extract the published outcome before matching: the `Ref`
            // from `wait_for` borrows `rx` mutably, so `rx.borrow()` cannot
            // run while it lives.
            let published = match rx.wait_for(|outcome| outcome.is_some()).await {
                Ok(guard) => guard.clone().expect("wait_for observed Some"),
                Err(_) => {
                    // The flight task died without publishing (panic/abort);
                    // the `Lead` drop freed the slot, so the next `enter`
                    // elects a new leader. Retry once, then surface the
                    // failure instead of spinning on a poisoned fetch.
                    if re_elected {
                        break Err(PipelineError::AllEnginesFailed(
                            runnable
                                .iter()
                                .map(|e| {
                                    (
                                        e.id(),
                                        EngineError::Transport(
                                            "in-flight search task vanished".to_string(),
                                        ),
                                    )
                                })
                                .collect(),
                        ));
                    }
                    re_elected = true;
                    warn!("admission: flight vanished before publishing; re-electing");
                    continue;
                }
            };
            break published;
        };
        self.shared_response(req, &key, &runnable, shared, request_id, started)
            .await
    }

    /// Spawn the leader's flight task: tier-2 lexical lookup, then per-engine
    /// permits under the wait budget, then [`SearchPipeline::fetch`]; on
    /// overflow [`SearchPipeline::overflow`] serves a stored row or
    /// `RateLimited`. The task is detached from the caller's future on
    /// purpose: it holds a `SearchPipeline` clone, so a client disconnect
    /// mid-flight still lets the fetch finish, publish to remaining waiters
    /// and persist.
    fn spawn_flight(
        &self,
        lead: Lead,
        req: SearchRequest,
        key: CacheKey,
        runnable: Vec<Arc<dyn Engine>>,
        ttl: Duration,
        request_id: Uuid,
    ) {
        let pipe = self.clone();
        let span = info_span!(
            "admission.flight",
            request_id = %request_id,
            key = %key,
            engines = runnable.len(),
        );
        tokio::spawn(
            async move {
                pipe.flight(lead, req, key, runnable, ttl, request_id).await;
            }
            .instrument(span),
        );
    }

    /// The flight body: tier-2 lexical lookup, bounded permit wait ->
    /// fan-out -> persist -> publish; on overflow a stored row (fresh or
    /// stale) or `RateLimited`, and a stale serve enqueues a background
    /// refresh.
    async fn flight(
        &self,
        lead: Lead,
        req: SearchRequest,
        key: CacheKey,
        runnable: Vec<Arc<dyn Engine>>,
        ttl: Duration,
        request_id: Uuid,
    ) {
        let started = Instant::now();

        // ---- tier-2 lexical lookup (W1-10), inside the flight ----------
        // One FTS query per flight — followers share the outcome — and a
        // hit never spends an engine permit. Runs before the engine check
        // so a tier-2 hit serves even when nothing is configured (the
        // pre-admission ordering).
        if self.lexical.enabled
            && let Some(hit) = self
                .lexical_lookup(&req, &normalize_query(&req.q), request_id)
                .await
        {
            lead.complete(Ok(Arc::new(self.cache_hit_response(
                hit,
                Tier::T2,
                request_id,
                started,
                false,
            ))));
            return;
        }
        if runnable.is_empty() {
            warn!(pinned = ?req.engines, "no engines to run");
            lead.complete(Err(PipelineError::NoEngines));
            return;
        }

        let ids: Vec<EngineId> = runnable.iter().map(|e| e.id()).collect();
        // `oxe_admission_wait_ms` measures the bounded-queue wait; the
        // singleflight join in `run()` is not queueing and stays uncounted.
        let queued = Instant::now();
        let permits = self.admission.acquire(&ids).await;
        self.metrics.record_admission_wait(queued.elapsed());
        let outcome = match permits {
            Ok(_permits) => self
                .fetch(&req, &runnable, &key, ttl, request_id, started)
                .await
                .map(Arc::new),
            Err(_) => self.overflow(&key, request_id, started).await,
        };
        let served_stale = matches!(
            &outcome,
            Ok(resp) if matches!(resp.meta.source, Source::Cache { stale: true, .. })
        );
        lead.complete(outcome);
        // The refresh is spawned *after* `complete` cleared the slot so it
        // can elect itself; an `enter` while our own entry still existed
        // would make it join (and skip) the flight it came to replace.
        if served_stale {
            self.spawn_refresh(req, key, runnable, ttl);
        }
    }

    /// Enqueue a background refresh for a stale-served key. The refresh
    /// queues for engine permits with a generous budget (nobody is blocked
    /// on it) and only elects itself once it can run immediately, so a
    /// waiting refresh never holds a flight slot that real requests would
    /// join. If another flight for the key registered meanwhile, the
    /// refresh is redundant and exits.
    fn spawn_refresh(
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
                let ids: Vec<EngineId> = runnable.iter().map(|e| e.id()).collect();
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
    async fn overflow(
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

    fn rate_limited(&self) -> PipelineError {
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
    async fn shared_response(
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
                // serve or a deadline hit is observed by each waiter.
                self.metrics
                    .record_search(&req.client, label, tier, started.elapsed());
                if matches!(resp.meta.source, Source::Cache { stale: true, .. }) {
                    self.metrics.record_stale_served();
                }
                if resp.meta.deadline_hit {
                    self.metrics.record_deadline_hit();
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
                self.metrics
                    .record_search(&req.client, "network", None, started.elapsed());
                // A 429 reached the client: one rejection per waiter.
                // `queue_full` matches the reason label in `rate_limited`.
                if matches!(e, PipelineError::RateLimited { .. }) {
                    self.metrics.record_admission_rejected("queue_full");
                }
                Err(e)
            }
        }
    }

    /// One flight's upstream work: parallel fan-out under the hard
    /// deadline, per-engine reports, RRF merge, persist, response. The
    /// `search_log` write is deliberately absent — each waiter writes its
    /// own row in [`SearchPipeline::shared_response`].
    async fn fetch(
        &self,
        req: &SearchRequest,
        runnable: &[Arc<dyn Engine>],
        key: &CacheKey,
        ttl: Duration,
        request_id: Uuid,
        started: Instant,
    ) -> Result<SearchResponse, PipelineError> {
        let outcomes = self.fan_out(req, runnable, request_id).await;

        // ---- per-engine reports ------------------------------------------
        let mut reports: Vec<(usize, EngineReport)> = Vec::with_capacity(runnable.len());
        let mut ok_results: Vec<(usize, Vec<SearchResult>)> = Vec::new();
        let mut failures: Vec<(usize, EngineId, EngineError)> = Vec::new();
        let mut deadline_hit = false;
        let mut answered = vec![false; runnable.len()];
        let mut ttfr_recorded = false;

        for outcome in outcomes {
            let idx = outcome.idx;
            answered[idx] = true;
            let latency_ms = millis(outcome.latency);
            match outcome.outcome {
                Ok(Ok(results)) => {
                    self.metrics.record_engine_call(
                        &outcome.id,
                        "ok",
                        outcome.latency,
                        Some(results.len()),
                    );
                    if !ttfr_recorded {
                        // Time to first engine result, measured from the
                        // search start (includes the cache-miss lookup).
                        self.metrics.record_ttfr(started.elapsed());
                        ttfr_recorded = true;
                    }
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
                    // A completed call with an empty answer.
                    self.metrics.record_engine_call(
                        &outcome.id,
                        "no_results",
                        outcome.latency,
                        Some(0),
                    );
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
                    self.metrics.record_engine_call(
                        &outcome.id,
                        engine_error_label(&err),
                        outcome.latency,
                        None,
                    );
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
                    self.metrics
                        .record_engine_call(&outcome.id, "timeout", outcome.latency, None);
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
            self.metrics
                .record_engine_call(&id, "transport", started.elapsed(), None);
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
            // No log write here: every waiter on the flight logs its own
            // failure row in `shared_response`.
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
            query: normalize_query(&req.q),
            results: merged,
            meta: SearchMeta {
                source: Source::Network,
                engines_used,
                deadline_hit,
                elapsed_ms: millis(started.elapsed()),
                request_id,
            },
        };

        // ---- persist -------------------------------------------------------
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
            .put(key, &resp, ttl)
            .instrument(persist.clone())
            .await;
        persist.in_scope(|| match put {
            Ok(()) => debug!("response cached"),
            Err(e) => warn!(error = %e, "cache write failed; serving response anyway"),
        });
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
    async fn lexical_lookup(
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

    /// Rebuild a stored row as a cache response: provenance
    /// (`engines_used`, `query`) is kept while `source`, `elapsed_ms` and
    /// `request_id` describe this request. `ttl_s` is the remaining TTL
    /// (0 on an expired row); `stale` marks rows past `expires_at` served
    /// by the admission-overflow path (W1-07). Fuzzy tiers (2+) carry
    /// `matched_query` (the stored query); an exact tier-1 hit leaves it
    /// `None`.
    fn cache_hit_response(
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
