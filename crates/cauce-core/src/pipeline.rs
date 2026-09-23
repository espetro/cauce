//! `SearchPipeline` v0 (W0-08, parent plan section 4.4 minus hedging
//! and the tier-3 lookup, which land in W3) plus the W1-06 breaker
//! gate, W1-07 admission control and the W1-10 tier-2 lexical lookup.
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
//!    the configured engines whose ids are in the set; any id outside the
//!    configured set rejects the request outright with
//!    [`PipelineError::UnknownEngines`] before the cache lookups (the
//!    strict `unknown_engines` contract, issue #90). The breaker gate
//!    ([`HealthTracker`]) skips `Open` engines and lets one probe through
//!    for `HalfOpen`; every outcome updates EWMA/failures and persists
//!    through `Store::put_health` (transitions urgent, rest debounced 1/s).
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
//! `cauce trace <id>` rebuilds the whole fan-out.
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

use tokio::sync::mpsc;

use crate::admission::{Admission, FlightResult, Lead};
use crate::cache::{CacheKey, CachedSearch, lexical_tokens, normalize_query, token_jaccard};
use crate::config::LexicalConfig;
use crate::engine::{Engine, EngineError, EngineId, Tier};
use crate::health::{Gate, HealthPolicy, HealthTracker};
use crate::metrics::{Metrics, engine_error_label};
use crate::normalize::normalize_url;
use crate::request::SearchRequest;
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source, StreamEvent,
    StreamMeta,
};
use crate::store::{BreakerState, LogSource, SearchLogRow, Store, StoreError};

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
    /// The pipeline was built with an empty engine list, or the request
    /// carried an empty `engines` pin (`Some([])`): nothing can run.
    ///
    /// A pin naming ids outside the configured set is rejected earlier as
    /// [`PipelineError::UnknownEngines`] (issue #90), so `NoEngines` from a
    /// non-empty pin is unreachable. W0-09 still maps the two cases
    /// differently: a bare `Some([])` pin is the caller's 400, an
    /// unconfigured pipeline a 503. Call
    /// [`SearchPipeline::configured_engines`] to distinguish (`0` means no
    /// engines configured).
    #[error("no engines available for this request")]
    NoEngines,
    /// The `engines` pin named ids that are not configured — the strict
    /// `unknown_engines` contract from issue #90, superseding wave-0's
    /// silent truncation of partial pins. Checked before any cache lookup
    /// or fan-out, so a stale pin is never served from a stored row.
    /// W0-09 maps it to 400 `unknown_engines`; W1-08 mirrors it as MCP
    /// `invalid_params`. The rendered message lists the rejected ids and
    /// the configured set, plus an edit-distance-1 "did you mean" hint
    /// when one applies.
    #[error("{}", render_unknown_engines(.unknown, .configured))]
    UnknownEngines {
        /// Pin ids that matched no configured engine (pin order, deduped).
        unknown: Vec<EngineId>,
        /// Every configured engine id — the set a pin may name.
        configured: Vec<EngineId>,
    },
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
    /// Every engine that matched the request (pin applied) was skipped by
    /// an open circuit breaker — nothing was called. W1-06; the handler
    /// maps it to 503 `breaker_open` regardless of pinning.
    #[error("engines skipped by open breaker: {}", render_ids(.0))]
    BreakerOpen(Vec<EngineId>),
}

impl PipelineError {
    /// The per-engine failures of `AllEnginesFailed`; empty otherwise.
    pub fn failures(&self) -> &[(EngineId, EngineError)] {
        match self {
            Self::AllEnginesFailed(failures) => failures,
            Self::NoEngines
            | Self::UnknownEngines { .. }
            | Self::RateLimited { .. }
            | Self::BreakerOpen(_) => &[],
        }
    }
}

fn render_ids(ids: &[EngineId]) -> String {
    ids.iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `unknown_engines` message: rejected ids first, then the configured
/// set, then per-offender "did you mean" hints for edit-distance-1 ids
/// (the Stripe/ES precedent named in the #90 acceptance).
fn render_unknown_engines(unknown: &[EngineId], configured: &[EngineId]) -> String {
    let configured_list = if configured.is_empty() {
        "none".to_string()
    } else {
        render_ids(configured)
    };
    let mut msg = format!(
        "unknown engine ids: {}; configured engines: {configured_list}",
        render_ids(unknown),
    );
    let hints: Vec<String> = unknown
        .iter()
        .filter_map(|id| {
            configured
                .iter()
                .find(|c| edit_distance_one(id.as_str(), c.as_str()))
                .map(|c| format!("{id} -> {c}"))
        })
        .collect();
    if !hints.is_empty() {
        msg.push_str(&format!("; did you mean {}?", hints.join(", ")));
    }
    msg
}

/// `true` when `a` is one insertion, deletion or substitution away from
/// `b` — the did-you-mean bar for engine ids.
fn edit_distance_one(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    match a.len().abs_diff(b.len()) {
        0 => a.iter().zip(&b).filter(|(x, y)| x != y).count() == 1,
        1 => {
            let (short, long) = if a.len() < b.len() {
                (&a, &b)
            } else {
                (&b, &a)
            };
            // Skip at most one char of `long`; every `short` char must
            // match in order.
            let mut i = 0;
            let mut j = 0;
            let mut skipped = false;
            while i < short.len() && j < long.len() {
                if short[i] == long[j] {
                    i += 1;
                } else if skipped {
                    return false;
                } else {
                    skipped = true;
                }
                j += 1;
            }
            true
        }
        _ => false,
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

/// Fan-out bookkeeping shared by the collect and incremental stream paths.
struct FanOut {
    answered: Vec<bool>,
    reports: Vec<(usize, EngineReport)>,
    failures: Vec<(usize, EngineId, EngineError)>,
    deadline_hit: bool,
    had_answer: bool,
    ttfr_recorded: bool,
    started_at: Instant,
}

impl FanOut {
    fn new(engines: usize, started_at: Instant) -> Self {
        Self {
            answered: vec![false; engines],
            reports: Vec::with_capacity(engines),
            failures: Vec::new(),
            deadline_hit: false,
            had_answer: false,
            ttfr_recorded: false,
            started_at,
        }
    }
}

/// Borrowed context shared by the fetch completion stages.
struct FetchCtx<'a> {
    req: &'a SearchRequest,
    key: &'a CacheKey,
    ttl: Duration,
    request_id: Uuid,
    started: Instant,
}

/// Shared borrowed state for a progressive search flight.
struct StreamCtx<'a> {
    req: &'a SearchRequest,
    key: &'a CacheKey,
    ttl: Duration,
    request_id: Uuid,
    started: Instant,
    tx: &'a mpsc::UnboundedSender<StreamEvent>,
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
    /// Per-engine EWMA/breaker state (W1-06): consulted before fan-out and
    /// updated from every outcome.
    health: Arc<HealthTracker>,
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
        let health = Arc::new(HealthTracker::new(store.clone()));
        for engine in &engines {
            health.register(&engine.id());
        }
        Self {
            store,
            engines,
            health,
            deadline: DEFAULT_DEADLINE,
            default_ttl: DEFAULT_TTL,
            ttl_cap: DEFAULT_TTL_CAP,
            lexical: LexicalConfig::default(),
            admission: Admission::default(),
            metrics: Metrics,
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

    /// Override the breaker policy (breaker windows, timeout threshold).
    /// Tests shrink the windows instead of sleeping minutes; production
    /// uses [`HealthPolicy::default`].
    pub fn with_health_policy(mut self, policy: HealthPolicy) -> Self {
        let health = Arc::new(HealthTracker::with_policy(self.store.clone(), policy));
        for engine in &self.engines {
            health.register(&engine.id());
        }
        self.health = health;
        self
    }

    /// The live per-engine health tracker (EWMA, breaker state) — also
    /// what `GET /api/engines` and the reset route operate on, and what
    /// the W1-09 metrics surface polls via `snapshot()`.
    pub fn health(&self) -> &Arc<HealthTracker> {
        &self.health
    }

    /// Load persisted `engine_health` rows into the tracker (startup, plan
    /// 4.4.6: a restart must not hammer a blocked engine). Returns the
    /// number of rows applied.
    pub async fn load_health(&self) -> Result<usize, StoreError> {
        self.health.load().await
    }

    /// Number of configured engines. `0` means the pipeline is
    /// unconfigured. W0-09 maps `NoEngines` + (zero configured engines or
    /// an empty `engines` pin) to 503 `no_engines`, and
    /// [`PipelineError::UnknownEngines`] — a pin naming any id outside this
    /// set — to 400 `unknown_engines` regardless of the configured count
    /// (issue #90 strict contract).
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

    /// Streaming variant of [`SearchPipeline::search_opts`] (W2-01, parent
    /// plan 4.4 step 7): engine batches are yielded as calls return
    /// instead of waiting for the whole fan-out. Same cache/admission/
    /// breaker semantics as `search`:
    ///
    /// - the engine pin is validated *before* the stream task spawns, so a
    ///   rejected pin is a synchronous [`PipelineError`] the inbound
    ///   surface can answer with its real status (400 `unknown_engines`
    ///   like `/api/search`) instead of a terminal `error` event on an
    ///   already-open stream;
    /// - a tier-1 hit emits one `results` batch per producing engine plus
    ///   the terminal `meta`, still writing its `search_log` row;
    /// - a miss elects a singleflight leader whose stream emits batches as
    ///   its engines return (merge state is kept incrementally and the
    ///   final RRF order goes out in `meta.order`); a follower joining an
    ///   in-flight flight emits the shared response once it publishes,
    ///   batched per producing engine;
    /// - pipeline failures past the pin check arrive as a terminal `error`
    ///   event; the inbound surface maps the typed [`PipelineError`] to
    ///   its wire code.
    ///
    /// The work runs in a spawned task holding a pipeline clone, so a
    /// dropped receiver (disconnected SSE client) still finishes the
    /// flight, persists, and publishes to followers. The channel closes
    /// after the terminal `meta`/`error` event.
    pub async fn search_stream(
        &self,
        req: &SearchRequest,
        opts: SearchOpts,
    ) -> Result<mpsc::UnboundedReceiver<StreamEvent>, PipelineError> {
        let request_id = opts.request_id.unwrap_or_else(Uuid::now_v7);
        let span = info_span!(
            "pipeline.search",
            request_id = %request_id,
            query = %normalize_query(&req.q),
            page = req.page,
            client = %req.client,
            // Runnable count (post-pin) is recorded once known.
            engines = tracing::field::Empty,
            streaming = true,
        );
        self.validate_pin(req).instrument(span.clone()).await?;
        let (tx, rx) = mpsc::unbounded_channel();
        let pipe = self.clone();
        let req = req.clone();
        tokio::spawn(
            async move { pipe.run_stream(&req, opts.ttl, request_id, tx).await }.instrument(span),
        );
        Ok(rx)
    }

    /// Strict engine-pin validation, exposed for surfaces that must reject
    /// a bad pin without opening a stream (the `GET /search?stream=1`
    /// shell renders a 400 rather than a page that immediately errors).
    /// The rejected request still gets its `search_log` row, exactly like
    /// [`SearchPipeline::run`].
    pub async fn validate_pin(&self, req: &SearchRequest) -> Result<(), PipelineError> {
        let started = Instant::now();
        let query = normalize_query(&req.q);
        let key = CacheKey::from(req);
        self.validate_engine_pin(req, &key, &query, started).await
    }

    /// Reject invalid engine pins before cache lookups on both the ordinary
    /// and streaming surfaces. The rejected request still receives its
    /// unconditional search-log row.
    async fn validate_engine_pin(
        &self,
        req: &SearchRequest,
        key: &CacheKey,
        query: &str,
        started: Instant,
    ) -> Result<(), PipelineError> {
        if let Some(ids) = &req.engines {
            let configured: Vec<EngineId> = self.engines.iter().map(|e| e.id()).collect();
            let mut unknown = Vec::new();
            for id in ids {
                if !configured.contains(id) && !unknown.contains(id) {
                    unknown.push(id.clone());
                }
            }
            if !unknown.is_empty() {
                let error = PipelineError::UnknownEngines {
                    unknown,
                    configured,
                };
                warn!(pinned = ?ids, error = %error, "unknown engine ids in pin");
                self.write_log(
                    req,
                    key,
                    query,
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
                self.metrics
                    .record_search(&req.client, "network", None, started.elapsed());
                return Err(error);
            }
        }
        Ok(())
    }

    /// The streaming request body: tier-1 hit, then the admission election
    /// — leaders run [`SearchPipeline::lead_stream`] (which emits events as
    /// it goes) while followers await the shared outcome and emit it at
    /// once. Every request lands in [`SearchPipeline::shared_response`] for
    /// its own `search_log` row and metrics, exactly like `run`. Pin
    /// validation is deliberately absent here: `search_stream` runs it
    /// before spawning so a rejected pin is a synchronous error, never an
    /// `error` event.
    async fn run_stream(
        &self,
        req: &SearchRequest,
        ttl_override: Option<Duration>,
        request_id: Uuid,
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) {
        let started = Instant::now();
        let query = normalize_query(&req.q);
        let key = CacheKey::from(req);

        // ---- tier-1 exact lookup ---------------------------------------
        if let Some(hit) = self.cache_lookup(&key, request_id).await {
            let engines = hit.engines.clone();
            let resp = self.cache_hit_response(hit, Tier::T1, request_id, started, false);
            self.emit_response(&tx, &resp, started);
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
            self.metrics
                .record_search(&req.client, "cache", Some(Tier::T1), started.elapsed());
            return;
        }

        // ---- fan-out set ---------------------------------------------------
        let runnable = self.runnable(req);
        // `pipeline.search` is the current span here (via `instrument`).
        tracing::Span::current().record("engines", runnable.len() as u64);

        // ---- admission: singleflight + bounded per-engine queue ----------
        // Same election contract as `run`: one leader per key does the
        // work, followers await the shared outcome. The leader emits its
        // own events inside `lead_stream`; a follower emits once, below.
        let ttl = ttl_override.unwrap_or(self.default_ttl).min(self.ttl_cap);
        let stream = StreamCtx {
            req,
            key: &key,
            ttl,
            request_id,
            started,
            tx: &tx,
        };
        let mut re_elected = false;
        let mut led = false;
        let shared: FlightResult = loop {
            let (lead, mut rx) = self.admission.enter(&key);
            if let Some(lead) = lead {
                led = true;
                break self.lead_stream(&stream, &runnable, lead).await;
            }
            match rx.wait_for(|outcome| outcome.is_some()).await {
                Ok(guard) => break guard.clone().expect("wait_for observed Some"),
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
            }
        };

        let res = self
            .shared_response(req, &key, &runnable, shared, request_id, started)
            .await;
        match res {
            // The leader already emitted its batches and `meta` inside
            // `lead_stream`; a follower emits the whole shared response now.
            Ok(resp) if !led => self.emit_response(&tx, &resp, started),
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e));
            }
        }
    }

    /// The streaming leader's flight body: the same stages as
    /// [`SearchPipeline::flight`] (tier-2 lexical, bounded permit wait with
    /// overflow fallback, fan-out, publish, stale refresh) but the fan-out
    /// is [`SearchPipeline::fetch_stream`], which pushes a `results` event
    /// per engine the moment its call resolves and the `meta` event once
    /// the merge is final. Returns the published [`FlightResult`]; error
    /// events are emitted by `run_stream` after `shared_response` so every
    /// failure still writes its `search_log` row first.
    async fn lead_stream(
        &self,
        stream: &StreamCtx<'_>,
        runnable: &[Arc<dyn Engine>],
        lead: Lead,
    ) -> FlightResult {
        // ---- tier-2 lexical lookup (W1-10) -------------------------------
        if self.lexical.enabled
            && let Some(hit) = self
                .lexical_lookup(
                    stream.req,
                    &normalize_query(&stream.req.q),
                    stream.request_id,
                )
                .await
        {
            let resp = Arc::new(self.cache_hit_response(
                hit,
                Tier::T2,
                stream.request_id,
                stream.started,
                false,
            ));
            self.emit_response(stream.tx, &resp, stream.started);
            lead.complete(Ok(resp.clone()));
            return Ok(resp);
        }
        if runnable.is_empty() {
            warn!(pinned = ?stream.req.engines, "no engines to run");
            let err = PipelineError::NoEngines;
            lead.complete(Err(err.clone()));
            return Err(err);
        }

        let ids: Vec<EngineId> = runnable.iter().map(|e| e.id()).collect();
        let queued = Instant::now();
        let permits = self.admission.acquire(&ids).await;
        self.metrics.record_admission_wait(queued.elapsed());
        let outcome: FlightResult = match permits {
            Ok(_permits) => self.fetch_stream(stream, runnable).await.map(Arc::new),
            Err(_) => {
                self.overflow(stream.key, stream.request_id, stream.started)
                    .await
            }
        };
        // A stored row served on overflow streams like a cache hit (the
        // network path already emitted its own events inside fetch_stream).
        if let Ok(resp) = &outcome
            && matches!(resp.meta.source, Source::Cache { .. })
        {
            self.emit_response(stream.tx, resp, stream.started);
        }
        let served_stale = matches!(
            &outcome,
            Ok(resp) if matches!(resp.meta.source, Source::Cache { stale: true, .. })
        );
        lead.complete(outcome.clone());
        // The refresh is spawned *after* `complete` cleared the slot so it
        // can elect itself, same as `flight`.
        if served_stale {
            self.spawn_refresh(
                stream.req.clone(),
                stream.key.clone(),
                runnable.to_vec(),
                stream.ttl,
            );
        }
        outcome
    }

    /// Emit a complete response as one `results` batch per producing
    /// engine plus the terminal `meta` — used by every non-incremental
    /// serve (tier-1/2 hits, stale overflow, singleflight followers). Batch
    /// order is `engines_used` fan-out order; a leftover batch labelled
    /// `merged` carries results whose producing engine is not in the
    /// reports (belt-and-braces for stored payloads).
    fn emit_response(
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
            order: resp.results.iter().map(|r| r.url.clone()).collect(),
        }));
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

        // ---- strict engine-pin validation before cache lookup (#90) -------
        self.validate_engine_pin(req, &key, &query, started).await?;

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
        // Pin first; the breaker gate (W1-06) runs inside `fetch` so a
        // claimed half-open probe is always followed by its call — a
        // tier-2 hit or an admission overflow never consumes the probe.
        let runnable = self.runnable(req);
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
            // Unreachable for a non-empty pin — unknown ids were rejected
            // in `run` as `UnknownEngines`. This is the unconfigured
            // pipeline (or a bare `Some([])` pin) case.
            warn!(pinned = ?req.engines, "no engines to run");
            lead.complete(Err(PipelineError::NoEngines));
            return;
        }

        let ids: Vec<EngineId> = runnable.iter().map(|e| e.id()).collect();
        // `cauce_admission_wait_ms` measures the bounded-queue wait; the
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
                // serve is observed by each waiter; the deadline hit was
                // already counted per flight in `fetch`.
                self.metrics
                    .record_search(&req.client, label, tier, started.elapsed());
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
        let (runnable, skipped) = self.breaker_gate(runnable, request_id);
        if runnable.is_empty() {
            // Nothing ran; `shared_response` still writes this request's
            // search_log row on the error path.
            return Err(PipelineError::BreakerOpen(skipped));
        }

        let mut fan = FanOut::new(runnable.len(), started);
        let mut ok_results: Vec<(usize, Vec<SearchResult>)> = Vec::new();
        for outcome in self.fan_out(req, &runnable, request_id).await {
            let idx = outcome.idx;
            if let Some(results) = self.fold_outcome(&mut fan, outcome, started) {
                ok_results.push((idx, results));
            }
        }
        let merged = self.merge_outcomes(&runnable, &mut fan, ok_results, request_id);

        let ctx = FetchCtx {
            req,
            key,
            ttl,
            request_id,
            started,
        };
        self.finish_fetch(skipped, fan, merged, ctx).await
    }

    /// The streaming fan-out (W2-01): identical stages to
    /// [`SearchPipeline::fetch`] — breaker gate, per-engine reports,
    /// health, merge, persist — but each engine's page is pushed as a
    /// `results` event the moment its call resolves, merge state is kept
    /// incrementally in an [`RrfMerge`], and the terminal `meta` event
    /// carries the final RRF `order` (the same list `resp.results` holds).
    async fn fetch_stream(
        &self,
        stream: &StreamCtx<'_>,
        runnable: &[Arc<dyn Engine>],
    ) -> Result<SearchResponse, PipelineError> {
        let (runnable, skipped) = self.breaker_gate(runnable, stream.request_id);
        if runnable.is_empty() {
            return Err(PipelineError::BreakerOpen(skipped));
        }

        let mut fan = FanOut::new(runnable.len(), stream.started);
        let mut merge = RrfMerge::new();
        self.fan_out_stream(stream, &runnable, &mut fan, &mut merge)
            .await;
        self.reconcile_unanswered(&runnable, &mut fan, stream.request_id);
        let merge_span = info_span!(
            "merge",
            request_id = %stream.request_id,
            r#in = tracing::field::Empty,
            out = tracing::field::Empty,
            deadline_hit = fan.deadline_hit,
        );
        let merged = {
            let _entered = merge_span.enter();
            let raw = merge.raw_count;
            let merged = merge.finish();
            merge_span.record("in", raw as u64);
            merge_span.record("out", merged.len() as u64);
            debug!(
                raw,
                merged = merged.len(),
                deadline_hit = fan.deadline_hit,
                "merged results"
            );
            merged
        };
        let fetch = FetchCtx {
            req: stream.req,
            key: stream.key,
            ttl: stream.ttl,
            request_id: stream.request_id,
            started: stream.started,
        };
        let resp = self.finish_fetch(skipped, fan, merged, fetch).await?;
        let _ = stream.tx.send(StreamEvent::Meta(StreamMeta {
            meta: resp.meta.clone(),
            order: resp.results.iter().map(|r| r.url.clone()).collect(),
        }));
        Ok(resp)
    }

    /// The breaker gate (W1-06), extracted so `fetch` and `fetch_stream`
    /// share it verbatim: `Open` engines are skipped, `HalfOpen` admits
    /// exactly one probe. Gated here — after the permit wait, immediately
    /// before fan-out — so a claimed probe is always followed by its call:
    /// the tier-2 hit and overflow paths return before this point and
    /// never consume the single probe slot. Returns the admitted engines
    /// (fan-out order) and the ids the open breakers suppressed.
    fn breaker_gate(
        &self,
        runnable: &[Arc<dyn Engine>],
        request_id: Uuid,
    ) -> (Vec<Arc<dyn Engine>>, Vec<EngineId>) {
        let mut gated: Vec<Arc<dyn Engine>> = Vec::with_capacity(runnable.len());
        let mut skipped: Vec<EngineId> = Vec::new();
        for engine in runnable {
            match self.health.admission(&engine.id(), request_id) {
                Gate::Call | Gate::Probe => gated.push(engine.clone()),
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
    fn fold_outcome(
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

    /// Final merge for the non-streaming fan-out: fill the slots whose
    /// tasks never answered (a `JoinError` — panic/cancel — never names
    /// its engine), then dedupe-by-URL + RRF in fan-out order.
    fn merge_outcomes(
        &self,
        runnable: &[Arc<dyn Engine>],
        fan: &mut FanOut,
        mut ok_results: Vec<(usize, Vec<SearchResult>)>,
        request_id: Uuid,
    ) -> Vec<SearchResult> {
        self.reconcile_unanswered(runnable, fan, request_id);
        ok_results.sort_by_key(|(idx, _)| *idx);
        let mut merge = RrfMerge::new();
        for (idx, results) in &ok_results {
            merge.add(*idx, results);
        }
        let raw = merge.raw_count;
        let span = info_span!(
            "merge",
            request_id = %request_id,
            r#in = tracing::field::Empty,
            out = tracing::field::Empty,
            deadline_hit = fan.deadline_hit,
        );
        let _e = span.enter();
        let merged = merge.finish();
        span.record("in", raw as u64);
        span.record("out", merged.len() as u64);
        debug!(
            raw,
            merged = merged.len(),
            deadline_hit = fan.deadline_hit,
            "merged results"
        );
        merged
    }

    /// Mark panicked/cancelled engine tasks as transport failures. A JoinError
    /// does not carry its engine id, so unanswered slots identify them.
    fn reconcile_unanswered(
        &self,
        runnable: &[Arc<dyn Engine>],
        fan: &mut FanOut,
        request_id: Uuid,
    ) {
        for (idx, engine) in runnable.iter().enumerate() {
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

    /// Everything after the fan-out both `fetch` variants share: report
    /// ordering, deadline metric, health flush, the all-failed error,
    /// response assembly (with breaker-skipped ids in `engines_skipped`,
    /// W2-01), and the cache persist. `merged` is the final RRF list —
    /// computed incrementally on the stream path, batch-sorted here on the
    /// collect path.
    async fn finish_fetch(
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

    /// Spawn one bounded-deadline task per engine. Both result collectors
    /// share this path so streaming cannot diverge in health or error rules.
    fn spawn_engine_calls(
        &self,
        req: &SearchRequest,
        runnable: &[Arc<dyn Engine>],
        request_id: Uuid,
    ) -> tokio::task::JoinSet<EngineOutcome> {
        let mut set = tokio::task::JoinSet::new();
        for (idx, engine) in runnable.iter().enumerate() {
            let engine = engine.clone();
            let id = engine.id();
            let deadline = self.deadline;
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
        set
    }

    /// Collect outcomes after the fan-out barrier (non-streaming search).
    async fn fan_out(
        &self,
        req: &SearchRequest,
        runnable: &[Arc<dyn Engine>],
        request_id: Uuid,
    ) -> Vec<EngineOutcome> {
        let mut set = self.spawn_engine_calls(req, runnable, request_id);
        let mut outcomes = Vec::with_capacity(runnable.len());
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(outcome) => outcomes.push(outcome),
                Err(join_err) => warn!(error = %join_err, "engine task failed to join"),
            }
        }
        outcomes
    }

    /// Fold and emit each engine as soon as its task completes. The shared
    /// `RrfMerge` keeps contribution and stable tie-break state by engine
    /// index, so arrival order never changes the terminal ranking.
    async fn fan_out_stream(
        &self,
        stream: &StreamCtx<'_>,
        runnable: &[Arc<dyn Engine>],
        fan: &mut FanOut,
        merge: &mut RrfMerge,
    ) {
        let mut set = self.spawn_engine_calls(stream.req, runnable, stream.request_id);
        while let Some(joined) = set.join_next().await {
            let outcome = match joined {
                Ok(outcome) => outcome,
                Err(join_err) => {
                    warn!(error = %join_err, "engine task failed to join");
                    continue;
                }
            };
            let idx = outcome.idx;
            let engine = outcome.id.clone();
            if let Some(results) = self.fold_outcome(fan, outcome, stream.started) {
                if results.is_empty() {
                    continue;
                }
                merge.add(idx, &results);
                let _ = stream.tx.send(StreamEvent::Results {
                    engine,
                    results,
                    elapsed_ms: millis(stream.started.elapsed()),
                });
            }
        }
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
            engines_skipped: Vec::new(),
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

/// Fields of a `SearchLogRow` the call site supplies; `write_log` fills
/// `id`, `ts`, `query_hash`, `query`, `client` and `latency_ms`.
struct LogRow {
    source: LogSource,
    tier: Option<Tier>,
    result_count: u32,
    engines: Vec<EngineId>,
    deadline_hit: bool,
}

/// Incremental RRF accumulator. Contributions are keyed by engine index so
/// out-of-order completion never changes floating-point reduction or ties.
struct RrfMerge {
    map: HashMap<Url, RrfAcc>,
    raw_count: usize,
}

struct RrfAcc {
    result: SearchResult,
    contributions: std::collections::BTreeMap<usize, f32>,
    best: f32,
    best_order: (usize, usize),
    first_seen: (usize, usize),
}

impl RrfMerge {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            raw_count: 0,
        }
    }

    fn add(&mut self, engine_idx: usize, results: &[SearchResult]) {
        self.raw_count += results.len();
        for (rank0, result) in results.iter().enumerate() {
            let rank = rank0 + 1;
            let contribution = 1.0 / (RRF_K + rank0 as f32 + 1.0);
            // Dedupe on the normalized form but emit the engine's raw URL.
            let normalized = normalize_url(&result.url);
            let acc = self.map.entry(normalized).or_insert_with(|| RrfAcc {
                result: result.clone(),
                contributions: std::collections::BTreeMap::new(),
                best: contribution,
                best_order: (engine_idx, rank),
                first_seen: (engine_idx, rank),
            });
            *acc.contributions.entry(engine_idx).or_default() += contribution;
            acc.first_seen = acc.first_seen.min((engine_idx, rank));
            if contribution > acc.best
                || (contribution == acc.best && (engine_idx, rank) < acc.best_order)
            {
                acc.best = contribution;
                acc.best_order = (engine_idx, rank);
                acc.result.clone_from(result);
            }
        }
    }

    fn finish(self) -> Vec<SearchResult> {
        let mut merged: Vec<(f32, (usize, usize), SearchResult)> = self
            .map
            .into_values()
            .map(|acc| {
                let score = acc.contributions.values().copied().sum();
                (
                    score,
                    acc.first_seen,
                    SearchResult {
                        score,
                        ..acc.result
                    },
                )
            })
            .collect();
        merged.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        merged.into_iter().map(|(_, _, result)| result).collect()
    }
}
