//! `Store` trait and its row types (parent plan 4.2 + section 5 tables).
//!
//! One `Store` impl ships in wave 0 (`oxe-store-sqlite`, W0-04); the trait is
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
    pub query: String,
    pub client: ClientKind,
    pub source: LogSource,
    /// Cache tier that served the hit; `None` on `source = network`.
    pub tier: Option<Tier>,
    pub latency_ms: u32,
    pub result_count: u32,
    /// Engines that actually ran (`engines_json` column).
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
    /// 0-based position in the result list that was clicked.
    #[serde(default)]
    pub position: u32,
    #[serde(default)]
    pub client: ClientKind,
}

fn now() -> DateTime<Utc> {
    Utc::now()
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
/// Lives in `oxe-core` because the dependency direction is one-way toward
/// core: `oxe-core::config` produces these values and `oxe-store-sqlite`
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

/// Filters for `Store::list_history` (`GET /api/history?since&q&limit`).
#[derive(Debug, Clone)]
pub struct HistoryFilter {
    /// Only rows at or after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Substring match on the stored query (searches only; clicks are not
    /// filtered by it).
    pub q: Option<String>,
    /// Max items, newest first.
    pub limit: u32,
}

impl Default for HistoryFilter {
    /// Unfiltered, last 50 items.
    fn default() -> Self {
        Self {
            since: None,
            q: None,
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

/// Searches per UTC day, for the dashboard chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayCount {
    pub day: NaiveDate,
    pub searches: u64,
    pub cache_hits: u64,
}

/// Latency percentiles over `search_log.latency_ms` in the window.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub by_client: Vec<ClientCount>,
    /// Queries with `result_count = 0`, most frequent first.
    pub zero_result_queries: Vec<String>,
    pub per_day: Vec<DayCount>,
    /// Engine table: one row per known engine.
    pub engines: Vec<EngineHealthRow>,
    /// Live `cache_entries` rows (unexpired).
    pub cache_entries: u64,
    /// Rows past `expires_at` awaiting eviction.
    pub cache_entries_expired: u64,
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

    /// Delete all rows past `expires_at`; returns rows removed. Also the
    /// `DELETE /api/cache?expired=true` path (audited by the caller).
    async fn evict_expired(&self) -> Result<u64, StoreError>;

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
}
