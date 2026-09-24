//! `Store` trait and its row types (parent plan 4.2 + section 5 tables).
//!
//! One `Store` impl ships in wave 0 (`cauce-store-sqlite`, W0-04); the trait is
//! the settled contract the pipeline, routes and admin surfaces code against.
//! `get_semantic` (tier-3 vector cache) is deliberately absent: it lands with
//! the `semantic` feature in W5.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::Tier;
use crate::cache::{CacheKey, CachedSearch};
use crate::engine::EngineId;
use crate::request::ClientKind;
use crate::response::SearchResponse;

/// Errors returned by `Store` impls. Backend details (SQL, IO) are flattened
/// into `Backend`; malformed persisted data is `Corrupt`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store backend: {0}")]
    Backend(String),
    #[error("corrupt persisted data: {0}")]
    Corrupt(String),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// `search_log` row (section 5). Written unconditionally for every request,
/// cache hit or not. `id` is `None` on insert and `Some` when read back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLogRow {
    #[serde(default)]
    pub id: Option<i64>,
    pub ts: DateTime<Utc>,
    /// The `CacheKey` of the request; joins to `clicks.query_hash`.
    pub query_hash: CacheKey,
    /// The normalized query (`normalize_query`: lowercased, whitespace
    /// collapsed). Stays normalized because stats grouping and the
    /// history `q` filter depend on it.
    pub query: String,
    /// The query text exactly as submitted (original casing and spacing),
    /// for history displays. `None` on rows written before schema v2.
    #[serde(default)]
    pub query_raw: Option<String>,
    pub client: ClientKind,
    pub source: LogSource,
    /// Cache tier that served the hit; `None` on `source = network`.
    pub tier: Option<Tier>,
    pub latency_ms: u32,
    pub result_count: u32,
    /// Engines that actually ran (`engines_json` column). On cache-hit
    /// rows (`source = cache`) this instead carries the engines that
    /// *produced* the cached entry (provenance), not engines that ran for
    /// this request.
    pub engines: Vec<EngineId>,
    pub deadline_hit: bool,
}

/// `source` column of `search_log` (`cache` | `network`). Distinct from
/// `Source`, which additionally carries hit metadata on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    Cache,
    Network,
}

/// `clicks` row (section 5). Also the `POST /api/click` body: the beacon omits
/// `id`, `ts` and `client`, which the server fills.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickRow {
    #[serde(default)]
    pub id: Option<i64>,
    /// Server always fills this on write; the serde default exists only so the
    /// inbound beacon body can omit it.
    #[serde(default = "now")]
    pub ts: DateTime<Utc>,
    /// `CacheKey` of the search that produced the clicked result, when the
    /// client knows it.
    #[serde(default)]
    pub query_hash: Option<CacheKey>,
    pub url: Url,
    #[serde(default)]
    pub title: String,
    /// 0-based position in the result list that was clicked. Beacons sent
    /// through form-flattening extensions (htmx `json-enc`) stringify scalars,
    /// so `"0"` and `0` are both accepted.
    #[serde(default, deserialize_with = "u32_or_string")]
    pub position: u32,
    #[serde(default)]
    pub client: ClientKind,
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Accepts `0` or `"0"` — beacon clients that flatten params to strings
/// (htmx `hx-vals` + `json-enc`) cannot preserve number types.
fn u32_or_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        N(u32),
        S(String),
    }
    match NumOrStr::deserialize(d)? {
        NumOrStr::N(n) => Ok(n),
        NumOrStr::S(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

/// Breaker state persisted in `engine_health` (scheduler section 4.4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// `engine_health` row (section 5): scheduler state across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealthRow {
    pub engine: EngineId,
    /// EWMA latency in ms (`alpha = 0.3`, section 4.4.6).
    pub ewma_ms: f64,
    /// Consecutive failure counter.
    pub failures: u32,
    pub breaker: BreakerState,
    /// Open breaker re-closes (to half-open) at this instant.
    pub breaker_until: Option<DateTime<Utc>>,
    pub last_ok_at: Option<DateTime<Utc>>,
    /// Last `EngineError` rendered for display.
    pub last_error: Option<String>,
}

/// Storage tuning knobs derived from detected host resources
/// (`Resources::detect`, W0-11) and passed to a `Store` constructor.
///
/// Lives in `cauce-core` because the dependency direction is one-way toward
/// core: `cauce-core::config` produces these values and `cauce-store-sqlite`
/// consumes them. Defaults are conservative for a small machine; per settled
/// inputs, never a fixed large allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreTuning {
    /// SQLite `PRAGMA cache_size` budget in kibibytes.
    pub cache_size_kib: u32,
    /// SQLite `PRAGMA mmap_size` in bytes (0 disables mmap).
    pub mmap_size_bytes: u64,
    /// SQLite `PRAGMA busy_timeout` in milliseconds.
    pub busy_timeout_ms: u32,
}

impl Default for StoreTuning {
    /// Conservative baseline for a ~4-8 GB host; `Resources::detect` scales
    /// these up on bigger machines.
    fn default() -> Self {
        Self {
            cache_size_kib: 64 * 1024,
            mmap_size_bytes: 256 * 1024 * 1024,
            busy_timeout_ms: 5_000,
        }
    }
}

/// `audit` row (section 5): attributed admin/AI/MCP events.
///
/// `actor` follows the `ui | api | mcp:<client> | cli` convention
/// (`ClientKind::label()`), with an optional `X-Actor` override string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    #[serde(default)]
    pub id: Option<i64>,
    /// Server always fills this on write; the serde default exists only so a
    /// caller-built row can omit it.
    #[serde(default = "now")]
    pub ts: DateTime<Utc>,
    pub actor: String,
    /// e.g. `cache.delete`, `config.put`, `engine.reset`.
    pub action: String,
    /// What was acted on (cache key, engine id, config path).
    pub target: String,
    /// Free-form structured details (`details_json` column).
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default)]
    pub request_id: Option<Uuid>,
}

/// Filters for `Store::list_history` (`GET /api/history?since&q&cached&limit`).
#[derive(Debug, Clone)]
pub struct HistoryFilter {
    /// Only rows at or after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Substring match on the stored query (searches only; clicks are not
    /// filtered by it).
    pub q: Option<String>,
    /// `cached=1` (W2-02 amendment): only rows whose `query_hash` has a
    /// live (unexpired) `cache_entries` row.
    pub cached: bool,
    /// Max items, newest first.
    pub limit: u32,
}

impl Default for HistoryFilter {
    /// Unfiltered, last 50 items.
    fn default() -> Self {
        Self {
            since: None,
            q: None,
            cached: false,
            limit: 50,
        }
    }
}

/// One history item: a logged search or a click, merged newest-first by the
/// route (`GET /api/history` reads one plane: `search_log` + `clicks`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryItem {
    Search(SearchLogRow),
    Click(ClickRow),
}

/// Live cache state for one `query_hash` (the history page's `source`
/// column, W2-02 amendment): enough to render `cached · <age>` /
/// `cached · expired` and link to `/cache` without decoding payloads.
#[derive(Debug, Clone)]
pub struct CacheState {
    pub key: CacheKey,
    /// The stored query text — the `/cache?q=` link target.
    pub query: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Aggregates for the `/history` header line and cap note (W2-02).
#[derive(Debug, Clone, Copy, Default)]
pub struct HistoryStats {
    /// `search_log` rows in the trailing 24 hours.
    pub searches_24h: u64,
    /// `search_log` rows, all time.
    pub searches_total: u64,
    /// `clicks` rows since UTC midnight.
    pub clicks_today: u64,
    /// `search_log` rows matching the page's filters (ignoring `limit`):
    /// the `N` in "showing 200 of N".
    pub matching: u64,
}

/// Outcome of [`Store::delete_search_log`] (`DELETE /api/history/{id}`,
/// W2-02): enough context for the audit row without a second read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteSearchLog {
    /// The removed row's query text.
    pub query: String,
    /// `clicks` rows removed alongside it. Clicks cascade only when the
    /// deleted row was the last `search_log` row carrying its
    /// `query_hash` — a click belongs to the query, and a surviving row
    /// still displays them.
    pub clicks_removed: u64,
}

/// Filters for `Store::list_audit` (`GET /api/audit`).
#[derive(Debug, Clone)]
pub struct AuditFilter {
    pub since: Option<DateTime<Utc>>,
    pub actor: Option<String>,
    pub action: Option<String>,
    /// Max rows, newest first.
    pub limit: u32,
}

impl Default for AuditFilter {
    /// Unfiltered, last 50 rows.
    fn default() -> Self {
        Self {
            since: None,
            actor: None,
            action: None,
            limit: 50,
        }
    }
}

/// Distinct `actor`/`action` values the audit table has seen; feeds the
/// `/audit` page's filter `<select>`s.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditFacets {
    pub actors: Vec<String>,
    pub actions: Vec<String>,
}

/// Searches per UTC day, for the dashboard chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayCount {
    pub day: NaiveDate,
    pub searches: u64,
    pub cache_hits: u64,
}

/// Latency percentiles over `search_log.latency_ms` in the window (and the
/// in-process TTFR rolling window, W2-03).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50_ms: u32,
    pub p90_ms: u32,
    pub p99_ms: u32,
}

/// Request count for one `client` label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCount {
    pub client: String,
    pub searches: u64,
}

/// Request count for one stored `query` string (dashboard top-queries panel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCount {
    pub query: String,
    pub searches: u64,
}

/// Cache hits served by one tier (`search_log.tier` on `source = 'cache'`
/// rows). The dashboard renders the per-tier hit rate as `hits / searches`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierHit {
    pub tier: u8,
    pub hits: u64,
}

/// Median/p80/p95 of a millisecond sample window (engine phases, admission
/// waits). Zeroed when no samples exist — percentiles are always numbers on
/// the wire (the W1-09 acceptance contract).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PhaseStats {
    pub median_ms: u32,
    pub p80_ms: u32,
    pub p95_ms: u32,
}

/// One `/api/stats` `engines[]` row (W1-09): the persisted `engine_health`
/// fields plus the in-process request metrics merged in by the HTTP
/// handler (`StatsSnapshot::merge_metrics`). Store impls fill only the
/// health fields; the metric fields stay zeroed until the merge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatsRow {
    pub engine: EngineId,
    /// EWMA latency in ms (`engine_health.ewma_ms`; 0 when unseen).
    pub ewma_ms: f64,
    pub failures: u32,
    pub breaker: BreakerState,
    pub breaker_until: Option<DateTime<Utc>>,
    pub last_ok_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Engine calls completed this process lifetime.
    pub requests: u64,
    /// Total results returned across calls.
    pub result_count: u64,
    /// Successful answers / requests * 100 (SearXNG reliability parity;
    /// `no_results` answers count as successful).
    pub reliability_pct: f64,
    /// Whole-call percentiles (pipeline-observed latency).
    pub median_ms: u32,
    pub p80_ms: u32,
    pub p95_ms: u32,
    /// `cauce_engine_duration_ms{phase="http"}` samples (the fetch leg).
    pub http: PhaseStats,
    /// `cauce_engine_duration_ms{phase="parse"}` samples (extraction).
    pub parse: PhaseStats,
}

impl EngineStatsRow {
    /// Health-only row, as `Store::stats` produces it.
    pub fn from_health(row: EngineHealthRow) -> Self {
        Self {
            engine: row.engine,
            ewma_ms: row.ewma_ms,
            failures: row.failures,
            breaker: row.breaker,
            breaker_until: row.breaker_until,
            last_ok_at: row.last_ok_at,
            last_error: row.last_error,
            requests: 0,
            result_count: 0,
            reliability_pct: 0.0,
            median_ms: 0,
            p80_ms: 0,
            p95_ms: 0,
            http: PhaseStats::default(),
            parse: PhaseStats::default(),
        }
    }

    /// Metrics-only row for an engine with no `engine_health` row yet
    /// (pre-W1-06, every engine is here until its first health write).
    pub fn from_metrics(m: crate::metrics::EngineMetricStats) -> Self {
        let mut row = Self::from_health(EngineHealthRow {
            engine: m.engine.clone(),
            ewma_ms: 0.0,
            failures: 0,
            breaker: BreakerState::Closed,
            breaker_until: None,
            last_ok_at: None,
            last_error: None,
        });
        row.set_metrics(&m);
        row
    }

    /// Fill the metric fields from the in-process aggregates.
    pub fn set_metrics(&mut self, m: &crate::metrics::EngineMetricStats) {
        self.requests = m.requests;
        self.result_count = m.result_count;
        self.reliability_pct = m.reliability_pct;
        self.median_ms = m.total.median_ms;
        self.p80_ms = m.total.p80_ms;
        self.p95_ms = m.total.p95_ms;
        self.http = m.http;
        self.parse = m.parse;
    }
}

/// Admission/queue aggregates for `/api/stats` (W1-09). Sourced from the
/// in-process metrics registry by `StatsSnapshot::merge_metrics`; store
/// impls emit it zeroed. Counts are per request (every waiter on a flight
/// observes the same rejection/stale outcome) except `deadline_hits`, which
/// is per flight — the deadline cuts the shared fan-out once.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AdmissionStats {
    /// Flight leader acquires that measured a bounded-queue wait
    /// (`cauce_admission_wait_ms` observations).
    pub waits: u64,
    pub wait_median_ms: u32,
    pub wait_p80_ms: u32,
    pub wait_p95_ms: u32,
    /// Requests rejected by admission (`cauce_admission_rejected_total`).
    pub rejected: u64,
    /// Rejections split by `reason` label (`queue_full`, `wait_timeout`).
    pub rejected_by_reason: std::collections::BTreeMap<String, u64>,
    /// Flights cut by the hard deadline (`cauce_deadline_hit_total`,
    /// per flight — shared across the flight's waiters).
    pub deadline_hits: u64,
    /// Responses served from an expired cache row
    /// (`cauce_stale_served_total`).
    pub stale_served: u64,
}

/// Dashboard aggregates (`GET /api/stats?days`, section 5 search_log readers
/// plus the cache/engine panels).
///
/// `hit_rate` is `cache_hits / searches` over `search_log`, never derived from
/// `cache_entries` (that was the v2 dashboard bug).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSnapshot {
    pub window_days: u32,
    pub searches: u64,
    /// Searches whose `source` was `cache`.
    pub cache_hits: u64,
    pub hit_rate: f64,
    /// None when the window has no searches.
    pub latency: Option<LatencyPercentiles>,
    /// TTFR percentiles (p50/p90/p99) over the in-process `cauce_ttfr_ms`
    /// rolling window (W2-03). Filled by `merge_metrics`; `None` before the
    /// first network search.
    pub ttfr: Option<LatencyPercentiles>,
    pub by_client: Vec<ClientCount>,
    /// Most frequent queries in the window, count descending (cap 10).
    pub top_queries: Vec<QueryCount>,
    /// Queries with `result_count = 0`, most frequent first.
    pub zero_result_queries: Vec<String>,
    /// Cache hits grouped by serving tier (W2-03 hit-rate-by-tier panel).
    pub hits_by_tier: Vec<TierHit>,
    /// Search outcome counts (`ok` / `error` / `rejected`) folded from
    /// `cauce_search_requests_total`'s `outcome` label (W2-03 amendment).
    /// Filled by `merge_metrics`; empty in store results.
    pub outcomes: std::collections::BTreeMap<String, u64>,
    /// Searches in the window that hit the hard deadline
    /// (`search_log.deadline_hit`). The windowed counterpart of the
    /// lifetime `admission.deadline_hits` counter: the reliability panel
    /// rates this against `searches`, never the lifetime counter, so
    /// numerator and denominator share the window.
    pub deadline_hits: u64,
    pub per_day: Vec<DayCount>,
    /// Engine table: one row per engine in `engine_health`, extended with
    /// the in-process request metrics by `merge_metrics` (W1-09).
    pub engines: Vec<EngineStatsRow>,
    /// Live `cache_entries` rows (unexpired).
    pub cache_entries: u64,
    /// Rows past `expires_at` awaiting eviction.
    pub cache_entries_expired: u64,
    /// Database file size in bytes (`page_count * page_size`; 0 when the
    /// store cannot report it).
    pub cache_db_bytes: u64,
    /// `created_at` of the newest `cache_entries` row.
    pub cache_newest_at: Option<DateTime<Utc>>,
    /// Admission/queue aggregates. Zeroed by store impls; the HTTP handler
    /// fills it from the in-process metrics registry via `merge_metrics`.
    pub admission: AdmissionStats,
    /// The newest `evals/results/<date>-engines.json` run (W3-05), read by
    /// the `/api/stats` handler on each request — `None` when no report
    /// file exists. Store impls always emit `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_eval: Option<crate::evals::EvalReport>,
}

impl StatsSnapshot {
    /// Overlay the in-process metrics (W1-09): health-derived `engines[]`
    /// rows gain requests/result_count/reliability and the latency
    /// percentiles; engines seen by the pipeline but missing from
    /// `engine_health` are appended; `admission` is filled. Called by the
    /// `/api/stats` handler, so `Store::stats` results stay health-only.
    pub fn merge_metrics(&mut self) {
        // Persisted `engine_health` rows predate the `EngineId` charset
        // check; an invalid id would render a dashboard row whose
        // `card_anchor` resolves to no card (`engine_views` filters the
        // same way).
        self.engines
            .retain(|r| EngineId::is_valid(r.engine.as_str()));
        for m in crate::metrics::engine_stats() {
            match self.engines.iter_mut().find(|r| r.engine == m.engine) {
                Some(row) => row.set_metrics(&m),
                None => self.engines.push(EngineStatsRow::from_metrics(m)),
            }
        }
        self.engines.sort_by(|a, b| a.engine.cmp(&b.engine));
        self.admission = crate::metrics::admission_stats();
        self.ttfr = crate::metrics::ttfr_percentiles();
        self.outcomes = crate::metrics::search_outcome_counts();
    }
}

/// Persistence contract (parent plan 4.2). Every table has exactly one writer
/// path through this trait (section 5); the pipeline calls it via
/// `tokio::task::spawn_blocking` in the sqlite impl.
///
/// Implementations must be `Send + Sync` and internally serialise writes
/// (single writer). All timestamps are `DateTime<Utc>`, ISO-8601 on the wire.
#[async_trait]
pub trait Store: Send + Sync {
    // ---- tier-1 exact cache -------------------------------------------------

    /// Fresh tier-1 lookup. Rows past `expires_at` are invisible here.
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError>;

    /// Tier-2 lexical lookup: FTS over stored queries and titles.
    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError>;

    /// Insert or replace the entry for `key` (`ttl` caps are applied by the
    /// pipeline, not the store).
    async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError>;

    /// Delete all rows past `expires_at + grace`; returns rows removed.
    /// `grace` keeps an expired row alive for its stale-serve window
    /// (W3-02, `cache.stale_grace_s`): a row that can still answer a
    /// request is not garbage. Also the `DELETE /api/cache?expired=true`
    /// path (audited by the caller).
    async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError>;

    // ---- cache admin (`/api/cache` routes, W0-09) ---------------------------

    /// List entries newest-first for the cache admin surface. Includes
    /// expired-but-not-yet-evicted rows.
    async fn list_cache(&self, limit: u32, offset: u32) -> Result<Vec<CachedSearch>, StoreError>;

    /// Fetch one entry by key regardless of expiry (admin `GET`).
    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError>;

    /// `DELETE /api/cache/{key}`. Returns false when the key did not exist.
    /// The caller writes the audit row.
    async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError>;

    /// `DELETE /api/cache?all=true`. Returns rows removed; audited by caller.
    async fn clear_cache(&self) -> Result<u64, StoreError>;

    // ---- search log, clicks, history -----------------------------------------

    /// Unconditional request log: called for every search, hit or not.
    async fn log_search(&self, row: SearchLogRow) -> Result<(), StoreError>;

    /// Click-through beacon (`POST /api/click`).
    async fn record_click(&self, row: ClickRow) -> Result<(), StoreError>;

    /// Combined history feed (`GET /api/history`): searches and clicks merged
    /// newest-first.
    async fn list_history(&self, filter: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError>;

    /// Cache state for a batch of `query_hash`es (W2-02 amendment): one row
    /// per present `cache_entries` row, expired rows included so the page can
    /// render `cached · expired`. The history page calls this once per
    /// request — batched, not per row.
    async fn cache_states(&self, keys: &[CacheKey]) -> Result<Vec<CacheState>, StoreError>;

    /// Which of `hashes` still have a `search_log` row (W2-02): the history
    /// page uses this to tell a true orphan click (`(click only)` row) from
    /// one whose search fell outside the rendered window — batched, one
    /// `IN` query per page.
    async fn search_hashes(&self, hashes: &[CacheKey]) -> Result<Vec<CacheKey>, StoreError>;

    /// The `/history` header aggregates and the filtered total behind the
    /// "showing N of M" cap note (W2-02).
    async fn history_stats(&self, filter: &HistoryFilter) -> Result<HistoryStats, StoreError>;

    /// `DELETE /api/history/{id}` (W2-02): remove one `search_log` row, plus
    /// the `clicks` rows sharing its `query_hash` only when no other
    /// `search_log` row carries that hash. Returns `None` when no row has
    /// that id. The caller writes the audit row.
    async fn delete_search_log(&self, id: i64) -> Result<Option<DeleteSearchLog>, StoreError>;

    // ---- stats --------------------------------------------------------------

    /// Dashboard aggregates over the trailing `days` (`GET /api/stats`).
    async fn stats(&self, days: u32) -> Result<StatsSnapshot, StoreError>;

    // ---- engine health -------------------------------------------------------

    /// All known engine health rows (`/api/engines`, engines page).
    async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError>;

    /// Insert or replace the health row for `row.engine` (scheduler updates
    /// EWMA/failures/breaker; `POST /api/engines/{id}/reset` writes a fresh
    /// `Closed` row).
    async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError>;

    // ---- audit ---------------------------------------------------------------

    /// Append an audit row. Called once per audited action, from the place the
    /// action happens.
    async fn audit(&self, row: AuditRow) -> Result<(), StoreError>;

    /// Audit rows newest-first (`GET /api/audit`).
    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditRow>, StoreError>;

    /// Distinct actors and actions present in the audit table (`/audit`
    /// filter dropdowns). Default: empty, for stores that do not persist
    /// audit rows.
    async fn audit_facets(&self) -> Result<AuditFacets, StoreError> {
        Ok(AuditFacets::default())
    }
}
