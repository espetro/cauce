//! `SearchPipeline` v0 (W0-08, parent plan section 4.4 minus the tier-3
//! lookup, which lands in W3) plus the W1-06 breaker gate, W1-07
//! admission control, the W1-10 tier-2 lexical lookup, the W3-01
//! P90 hedge to tier 2 and the W3-02 stale-while-revalidate serve.
//!
//! Module map (W3-08): `mod.rs` is the public API plus the request
//! lifecycle (admission, singleflight, leader/follower). The stages a
//! flight passes through live beside it: [`cache`] — tier-1/tier-2
//! lookups, stale overflow serve and background refresh; [`waves`] —
//! breaker gating and the primary/deferred split; [`fanout`] — wave
//! spawning, the hedge permit race and deadline enforcement; [`merge`]
//! — the RRF merge; [`archive`] — `search_archive`'s hybrid RRF over
//! `pages_fts` and `cache_fts` (W5-03); [`persistence`] — shared
//! response shaping, `put` and the search log.
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
//!    An *expired* hit inside `cache.stale_grace_s` (W3-02) is served the
//!    same way but `stale: true`, while a deduped background refresh
//!    re-fetches the key — and a stale serve during an all-breaker outage
//!    warns and counts `cauce_stale_served_total{reason=engines_unhealthy}`.
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
//!    spending an engine permit. Otherwise the flight waits for the
//!    t=0 wave's per-engine semaphore permits up to
//!    `admission.max_wait_ms` — a deferred (tier-2) engine's permit is
//!    acquired only inside the fetch, if its hedge actually triggers or
//!    promotion pulls it into the t=0 wave. A timed-out wait overflows
//!    to the stored row if one exists (fresh rows serve as a normal
//!    hit, expired rows go out `Source::Cache{stale:true}` and a
//!    background refresh is enqueued) else fails with
//!    [`PipelineError::RateLimited`].
//! 4. The flight fans out to every configured tier-1 (and specialised
//!    tier-3) engine in parallel ([`tokio::task::JoinSet`]), each call
//!    wrapped in `tokio::time::timeout(deadline)`; tier-2 is the W3-01
//!    hedge set: at `clamp(P90(tier-1 latency history), hedge floor,
//!    hedge ceiling)` it joins the fan-out on the remaining budget while
//!    fewer than `search.min_results` merged results have arrived
//!    (`meta.hedged`, `meta.hedge_at_ms`, `cauce_hedge_total{reason}`).
//!    Engines cut off at the deadline
//!    report `EngineStatus::Failed(EngineError::Timeout)` and set
//!    `meta.deadline_hit`. `req.engines = Some(ids)` pins the fan-out to
//!    the configured engines whose ids are in the set; any id outside the
//!    configured set rejects the request outright with
//!    [`PipelineError::UnknownEngines`] before the cache lookups (the
//!    strict `unknown_engines` contract, issue #90). The breaker gate
//!    ([`HealthTracker`]) skips `Open` engines and lets one probe through
//!    for `HalfOpen`; every outcome updates EWMA/failures and persists
//!    through `Store::put_health` (transitions urgent, rest debounced 1/s).
//! 5. Merge: dedupe by [`normalize_url`], RRF summed across engines
//!    (`score = sum weight/(k + rank)`, rank 1-based per engine; `k` is
//!    `merge.rrf_k` and `weight` the engine's observed reliability scaled
//!    into (0.5, 1.0], W3-03), stable ordering by first-seen position;
//!    the occurrence with the best single contribution supplies the
//!    emitted `SearchResult`. `merge.collapse_same_host_after` then caps
//!    emissions per host (default 3).
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

mod archive;
mod cache;
mod fanout;
mod merge;
mod persistence;
use persistence::LogRow;
mod waves;

pub use archive::{ArchiveHit, ArchiveSource};
pub use merge::{DEFAULT_COLLAPSE_SAME_HOST_AFTER, DEFAULT_RRF_K, RrfMerge};

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{Instrument, info, info_span, warn};
use uuid::Uuid;

use crate::admission::{Admission, FlightResult, Lead};
use crate::cache::{CacheKey, normalize_query};
use crate::config::LexicalConfig;
use crate::engine::{Engine, EngineError, EngineId, Tier};
use crate::health::{HealthPolicy, HealthTracker};
use crate::metrics::Metrics;
use crate::request::SearchRequest;
use crate::response::{SearchResponse, Source, StreamEvent};
use crate::store::{LogSource, Store, StoreError};

/// Hard fan-out deadline when the caller does not configure one
/// (`search.deadline_ms` in config once W0-11 lands).
pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(3_000);

/// Cache TTL for stored responses when no per-call override is passed.
pub const DEFAULT_TTL: Duration = Duration::from_secs(3_600);

/// Ceiling applied to every TTL, default or per-call override (24 h).
pub const DEFAULT_TTL_CAP: Duration = Duration::from_secs(86_400);

/// W3-01 hedge knobs (`search.hedge_floor_ms`,
/// `search.hedge_ceiling_ms`, `search.min_results`); see
/// [`SearchPipeline::with_hedge`].
#[derive(Debug, Clone)]
pub struct HedgePolicy {
    /// Earliest hedge point — tier-1 gets at least this long to answer.
    pub floor: Duration,
    /// Latest hedge point — a slow tier-1 history never delays the hedge
    /// past this.
    pub ceiling: Duration,
    /// Merged results wanted before the hedge is called off.
    pub min_results: usize,
}

/// Merge shaping (the `[merge]` config section, W3-03): the RRF constant
/// and the per-host emission cap. Per-engine reliability weights are not
/// configured here — the merge reads them per flight from
/// [`HealthTracker::reliability`].
///
/// [`HealthTracker::reliability`]: crate::health::HealthTracker::reliability
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MergePolicy {
    /// RRF `k` in `score += weight / (k + rank)` (`merge.rrf_k`).
    pub rrf_k: f32,
    /// Max merged results emitted per host — `m.`/`amp.` URL folds count
    /// toward the same host (`merge.collapse_same_host_after`); `0`
    /// disables the collapse.
    pub collapse_same_host_after: usize,
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self {
            rrf_k: DEFAULT_RRF_K,
            collapse_same_host_after: DEFAULT_COLLAPSE_SAME_HOST_AFTER,
        }
    }
}

/// W3-02 cache-hygiene knobs (`cache.stale_grace_s`,
/// `cache.degraded_ttl_s`); see [`SearchPipeline::with_cache_policy`].
#[derive(Debug, Clone, Copy)]
pub struct CachePolicy {
    /// How long past `expires_at` a tier-1 row may still be served stale
    /// while a deduped background refresh re-fetches it (default 6 h).
    /// `Duration::ZERO` disables the stale serve.
    pub stale_grace: Duration,
    /// TTL applied to a response whose fan-out was partial — any engine
    /// `Failed` or the deadline hit (default 60 s). A degraded answer
    /// never earns the full `default_ttl`.
    pub degraded_ttl: Duration,
}

impl Default for CachePolicy {
    /// The wave-3 settled defaults: 6 h stale grace, 60 s degraded TTL.
    fn default() -> Self {
        Self {
            stale_grace: Duration::from_secs(6 * 3_600),
            degraded_ttl: Duration::from_secs(60),
        }
    }
}

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

impl Default for HedgePolicy {
    /// The wave-3 settled defaults: floor 300 ms, ceiling 1500 ms,
    /// `min_results` 5.
    fn default() -> Self {
        Self {
            floor: Duration::from_millis(300),
            ceiling: Duration::from_millis(1_500),
            min_results: 5,
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

/// Borrowed context shared by the fetch completion stages.
pub(super) struct FetchCtx<'a> {
    pub(super) req: &'a SearchRequest,
    pub(super) key: &'a CacheKey,
    pub(super) ttl: Duration,
    pub(super) request_id: Uuid,
    pub(super) started: Instant,
}

/// Shared borrowed state for a progressive search flight.
pub(super) struct StreamCtx<'a> {
    pub(super) req: &'a SearchRequest,
    pub(super) key: &'a CacheKey,
    pub(super) ttl: Duration,
    pub(super) request_id: Uuid,
    pub(super) started: Instant,
    pub(super) tx: &'a mpsc::UnboundedSender<StreamEvent>,
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
    /// W3-01 hedge knobs (floor/ceiling on the P90 trigger, `min_results`).
    hedge: HedgePolicy,
    /// W3-03 merge shaping (`merge.rrf_k`, `merge.collapse_same_host_after`).
    merge: MergePolicy,

    /// W3-02 cache-hygiene knobs (stale-serve grace, degraded TTL).
    cache: CachePolicy,
    /// W1-09 metrics handle. A unit struct: every `record_*` writes into
    /// the process-global registry, so pipelines share one set of series.
    metrics: Metrics,
}

pub(super) fn millis(d: Duration) -> u32 {
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
            hedge: HedgePolicy::default(),
            merge: MergePolicy::default(),

            cache: CachePolicy::default(),
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

    /// Override the hedge policy (`search.hedge_floor_ms`,
    /// `search.hedge_ceiling_ms`, `search.min_results`, W3-01).
    /// Default: [`HedgePolicy::default`] (300 ms floor, 1500 ms ceiling,
    /// 5 results).
    pub fn with_hedge(mut self, hedge: HedgePolicy) -> Self {
        self.hedge = hedge;
        self
    }

    /// Override the merge policy (`merge.rrf_k`,
    /// `merge.collapse_same_host_after`, W3-03).
    /// Default: [`MergePolicy::default`] (k = 60, 3 per host).
    pub fn with_merge(mut self, merge: MergePolicy) -> Self {
        self.merge = merge;
        self
    }

    /// Override the cache-hygiene policy (`cache.stale_grace_s`,
    /// `cache.degraded_ttl_s`, W3-02). Default: [`CachePolicy::default`]
    /// (6 h stale-serve grace, 60 s TTL on degraded fan-outs).
    pub fn with_cache_policy(mut self, policy: CachePolicy) -> Self {
        self.cache = policy;
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

    /// The configured engine set as built (`id()`, `tier()`, `page_size()`).
    /// Read-only — the set is immutable for the process lifetime; display
    /// surfaces like the engines page read effective tiers from here.
    pub fn engines(&self) -> &[Arc<dyn Engine>] {
        &self.engines
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
                self.metrics.record_search(
                    &req.client,
                    "network",
                    None,
                    "error",
                    started.elapsed(),
                );
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
            self.metrics.record_search(
                &req.client,
                "cache",
                Some(Tier::T1),
                "ok",
                started.elapsed(),
            );
            return;
        }

        // ---- fan-out set ---------------------------------------------------
        let runnable = self.runnable(req);
        // `pipeline.search` is the current span here (via `instrument`).
        tracing::Span::current().record("engines", runnable.len() as u64);
        let ttl = ttl_override.unwrap_or(self.default_ttl).min(self.ttl_cap);

        // ---- stale-while-revalidate (W3-02) -------------------------------
        // An expired tier-1 row inside `cache.stale_grace_s` goes out
        // immediately `stale` while a deduped background refresh re-fetches
        // the key; same serve contract as `run`'s arm.
        if let Some(row) = self.stale_lookup(&key, request_id).await {
            let engines = row.engines.clone();
            let resp = self.cache_hit_response(row, Tier::T1, request_id, started, true);
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
                stale = true,
                results = resp.results.len(),
                elapsed_ms = resp.meta.elapsed_ms,
                "search complete"
            );
            self.metrics.record_search(
                &req.client,
                "cache",
                Some(Tier::T1),
                "ok",
                started.elapsed(),
            );
            self.metrics
                .record_stale_served(self.stale_reason(&runnable, "grace"));
            self.spawn_refresh(req.clone(), key.clone(), runnable.clone(), ttl);
            return;
        }

        // ---- admission: singleflight + bounded per-engine queue ----------
        // Same election contract as `run`: one leader per key does the
        // work, followers await the shared outcome. The leader emits its
        // own events inside `lead_stream`; a follower emits once, below.
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

        // Permits cover only the t=0 (non-tier-2) wave: a deferred
        // engine's permit is acquired when its hedge actually fires, so
        // saturated tier-2 capacity cannot block a flight whose
        // primaries need no hedge.
        let ids: Vec<EngineId> = runnable
            .iter()
            .filter(|e| e.tier() != Tier::T2)
            .map(|e| e.id())
            .collect();
        let queued = Instant::now();
        let permits = self.admission.acquire(&ids).await;
        self.metrics.record_admission_wait(queued.elapsed());
        let mut outcome: FlightResult = match permits {
            Ok(_permits) => self.fetch_stream(stream, runnable).await.map(Arc::new),
            Err(_) => {
                self.overflow(stream.key, stream.request_id, stream.started)
                    .await
            }
        };
        // A spawn-time acquire that timed out (promoted tier-2, or a
        // hedge that never won its permit) gets the same overflow
        // fallback as an exhausted primary queue.
        if matches!(outcome, Err(PipelineError::RateLimited { .. })) {
            outcome = self
                .overflow(stream.key, stream.request_id, stream.started)
                .await;
        }
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
                .record_search(&req.client, "cache", tier, "ok", started.elapsed());
            return Ok(resp);
        }

        // ---- fan-out set ---------------------------------------------------
        // Pin first; the breaker gate (W1-06) runs inside `fetch` so a
        // claimed half-open probe is always followed by its call — a
        // tier-2 hit or an admission overflow never consumes the probe.
        let runnable = self.runnable(req);
        // `pipeline.search` is the current span here (via `instrument`).
        tracing::Span::current().record("engines", runnable.len() as u64);
        let ttl = ttl_override.unwrap_or(self.default_ttl).min(self.ttl_cap);

        // ---- stale-while-revalidate (W3-02) -------------------------------
        // An expired tier-1 row inside `cache.stale_grace_s` is served
        // immediately as `Source::Cache{stale:true}` while a background
        // refresh re-fetches the key — deduped by the singleflight inside
        // `spawn_refresh`, so concurrent stale seekers collapse to one
        // refresh. Runs before admission: the stale serve needs no flight
        // slot, and a request served here never joins the watch channel.
        if let Some(row) = self.stale_lookup(&key, request_id).await {
            let engines = row.engines.clone();
            let resp = self.cache_hit_response(row, Tier::T1, request_id, started, true);
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
                stale = true,
                results = resp.results.len(),
                elapsed_ms = resp.meta.elapsed_ms,
                "search complete"
            );
            self.metrics.record_search(
                &req.client,
                "cache",
                Some(Tier::T1),
                "ok",
                started.elapsed(),
            );
            self.metrics
                .record_stale_served(self.stale_reason(&runnable, "grace"));
            self.spawn_refresh(req.clone(), key.clone(), runnable, ttl);
            return Ok(resp);
        }

        // ---- admission: singleflight + bounded per-engine queue (W1-07) ----
        //
        // Every miss for `key` elects one leader; its detached flight task
        // does the work below and publishes a `FlightResult`. Followers (and
        // the leader's own request) all wait on the same watch channel, so a
        // cancelled request can never strand work its twins are waiting on.
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

        // Only the t=0 wave's permits — see `lead_stream` for why
        // tier-2 is excluded.
        let ids: Vec<EngineId> = runnable
            .iter()
            .filter(|e| e.tier() != Tier::T2)
            .map(|e| e.id())
            .collect();
        // `cauce_admission_wait_ms` measures the bounded-queue wait; the
        // singleflight join in `run()` is not queueing and stays uncounted.
        let queued = Instant::now();
        let permits = self.admission.acquire(&ids).await;
        self.metrics.record_admission_wait(queued.elapsed());
        let mut outcome = match permits {
            Ok(_permits) => self
                .fetch(&req, &runnable, &key, ttl, request_id, started)
                .await
                .map(Arc::new),
            Err(_) => self.overflow(&key, request_id, started).await,
        };
        if matches!(outcome, Err(PipelineError::RateLimited { .. })) {
            outcome = self.overflow(&key, request_id, started).await;
        }
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
}
