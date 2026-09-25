//! `SqliteStore`: connection layout, pragmas and the `Store` impl.
//!
//! One writer connection behind a `tokio::sync::Mutex` owns every mutation
//! (WAL allows a single writer); reads go to a small round-robin pool of
//! connections each behind a `std::sync::Mutex`. Every rusqlite call runs in
//! `tokio::task::spawn_blocking` so the async runtime never blocks on disk.
//!
//! Module map: `mod.rs` keeps `SqliteStore`, the connection helpers
//! (`join_err`, `sql_err`, `like_pattern`, `fts_query`, `max_ts_ms`,
//! `percentile`) and the `Store` impl — each trait method a thin delegation
//! to a `pub(super)` inherent method in a sibling. [`cache`] —
//! `cache_entries` lookups, upsert, eviction, admin CRUD and the batched
//! `cache_states`/`search_hashes` history joins; [`answers`] — the W4-02 AI
//! answer cache (`answers` TTL read + upsert); [`history`] — `search_log`,
//! `clicks`, the merged feed and `history_stats`; [`stats`] — the
//! `/api/stats` aggregate groups; [`health`] — `engine_health` read/upsert;
//! [`pages`] — the W5-01 `pages` archive read + upsert; [`audit`] — `audit`
//! append, filtered list and facets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod answers;
mod audit;
mod cache;
mod health;
mod history;
mod pages;
mod stats;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cauce_core::{
    AnswerKey, AnswerRow, AuditFacets, AuditFilter, AuditRow, CacheKey, CacheState, CachedAnswer,
    CachedSearch, ClickRow, DeleteSearchLog, EngineHealthRow, HistoryFilter, HistoryItem,
    HistoryStats, PageRow, SearchLogRow, SearchResponse, StatsSnapshot, Store, StoreError,
    StoreTuning,
};
use rusqlite::Connection;
use tokio::task::{JoinError, spawn_blocking};

use crate::{migrate, rows};

/// Number of read connections in the pool. Reads are single-statement
/// lookups and WAL serialises nothing between readers, so a few is plenty;
/// no external pool dependency.
pub const READ_POOL_SIZE: usize = 3;

/// `Store` backed by a single SQLite file (WAL).
pub struct SqliteStore {
    writer: Arc<tokio::sync::Mutex<Connection>>,
    readers: Vec<Arc<Mutex<Connection>>>,
    next_reader: AtomicUsize,
}

fn join_err(e: JoinError) -> StoreError {
    StoreError::Backend(format!("blocking task failed: {e}"))
}

/// SQL failure -> `Backend`; a wrapped `StoreError` -> itself (decoders use
/// `rows::as_sql` to smuggle `Corrupt` through rusqlite closures).
fn sql_err(e: rusqlite::Error) -> StoreError {
    rows::as_store(e)
}

/// Escape a `LIKE ... ESCAPE '\'` prefix pattern: `s` must match from
/// position 0, with one trailing wildcard. Same metacharacter escaping
/// as [`like_pattern`]; SQLite `LIKE` gives the ASCII case-insensitive
/// match `suggest` wants.
fn like_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 1);
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Escape a free-text filter for `LIKE ... ESCAPE '\'` substring matching.
fn like_pattern(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('%');
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Build a safe FTS5 MATCH expression: each whitespace-separated term becomes
/// a quoted phrase, so user input can never inject FTS operators. Terms with
/// no alphanumeric character (e.g. `"*"`) tokenize to nothing and would raise
/// an FTS5 syntax error, so they are dropped; an all-punctuation query yields
/// `Ok(vec![])` rather than an error.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{t}\""))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Largest `expires_at` (ms) that decodes back to a `DateTime`: anything
/// bigger would make `from_timestamp_millis` fail and the row would decode as
/// `Corrupt` forever, so absurd TTLs clamp here instead of at `i64::MAX`.
fn max_ts_ms() -> i64 {
    chrono::DateTime::<chrono::Utc>::MAX_UTC.timestamp_millis()
}

/// Nearest-rank percentile of an ascending-sorted slice.
fn percentile(sorted: &[u32], p: u64) -> u32 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len() as u64;
    let rank = (n * p).div_ceil(100).max(1) - 1;
    sorted[(rank as usize).min(sorted.len() - 1)]
}

impl SqliteStore {
    /// Open (or create) the database at `path`, apply pragmas from `tuning`
    /// and run pending migrations.
    pub fn open(path: impl AsRef<Path>, tuning: StoreTuning) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if path == Path::new(":memory:") {
            // Each pooled connection would open its own private database and
            // reads would silently miss writes; use a temp file instead.
            return Err(StoreError::Backend(
                "SqliteStore does not support ':memory:' (per-connection private databases); pass a file path"
                    .to_string(),
            ));
        }
        let mut writer = Connection::open(path).map_err(sql_err)?;
        configure(&writer, &tuning, true)?;
        migrate::apply_pending(&mut writer)?;

        let mut readers = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let conn = Connection::open(path).map_err(sql_err)?;
            configure(&conn, &tuning, false)?;
            readers.push(Arc::new(Mutex::new(conn)));
        }

        Ok(Self {
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            readers,
            next_reader: AtomicUsize::new(0),
        })
    }

    /// Run `f` on the single writer connection, inside `spawn_blocking`.
    async fn with_writer<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.writer.clone().lock_owned().await;
        spawn_blocking(move || f(&conn)).await.map_err(join_err)?
    }

    /// Run `f` on the next round-robin reader, inside `spawn_blocking`.
    async fn with_reader<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
        T: Send + 'static,
    {
        let slot = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let reader = self.readers[slot].clone();
        spawn_blocking(move || {
            // A poisoned mutex only means a previous closure panicked; the
            // connection itself is still usable.
            let conn = reader.lock().unwrap_or_else(|e| e.into_inner());
            f(&conn)
        })
        .await
        .map_err(join_err)?
    }
}

/// Per-connection pragmas. `journal_mode` is database-persistent and set once
/// on the writer; `busy_timeout`, `cache_size` and `mmap_size` are
/// per-connection and set everywhere.
fn configure(conn: &Connection, tuning: &StoreTuning, wal: bool) -> Result<(), StoreError> {
    conn.busy_timeout(Duration::from_millis(u64::from(tuning.busy_timeout_ms)))
        .map_err(sql_err)?;
    // Negative cache_size is a KiB budget; positive would be a page count.
    conn.pragma_update(None, "cache_size", -i64::from(tuning.cache_size_kib))
        .map_err(sql_err)?;
    let mmap = tuning.mmap_size_bytes.min(i64::MAX as u64) as i64;
    conn.pragma_update(None, "mmap_size", mmap)
        .map_err(sql_err)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(sql_err)?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(sql_err)?;
    if wal {
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))
            .map_err(sql_err)?;
    }
    Ok(())
}

#[async_trait]
impl Store for SqliteStore {
    // ---- tier-1 exact cache -------------------------------------------------

    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        self.get_exact(key).await
    }

    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError> {
        self.get_lexical(q, limit).await
    }

    async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        self.put(key, resp, ttl).await
    }

    async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError> {
        self.evict_expired(grace).await
    }

    // ---- cache admin --------------------------------------------------------

    async fn list_cache(&self, limit: u32, offset: u32) -> Result<Vec<CachedSearch>, StoreError> {
        self.list_cache(limit, offset).await
    }

    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        self.get_cache(key).await
    }

    async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError> {
        self.delete_cache(key).await
    }

    async fn clear_cache(&self) -> Result<u64, StoreError> {
        self.clear_cache().await
    }

    // ---- answers ------------------------------------------------------------

    async fn get_answer(&self, key: &AnswerKey) -> Result<Option<CachedAnswer>, StoreError> {
        self.get_answer(key).await
    }

    async fn put_answer(
        &self,
        key: &AnswerKey,
        row: &AnswerRow,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        self.put_answer(key, row, ttl).await
    }

    // ---- pages --------------------------------------------------------------

    async fn put_page(&self, row: &PageRow) -> Result<(), StoreError> {
        self.put_page(row).await
    }

    async fn get_page(&self, url: &url::Url) -> Result<Option<PageRow>, StoreError> {
        self.get_page(url).await
    }

    // ---- search log, clicks, history ----------------------------------------

    async fn log_search(&self, row: SearchLogRow) -> Result<(), StoreError> {
        self.log_search(row).await
    }

    async fn record_click(&self, row: ClickRow) -> Result<(), StoreError> {
        self.record_click(row).await
    }

    async fn list_history(&self, filter: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError> {
        self.list_history(filter).await
    }

    async fn cache_states(&self, keys: &[CacheKey]) -> Result<Vec<CacheState>, StoreError> {
        self.cache_states(keys).await
    }

    async fn search_hashes(&self, hashes: &[CacheKey]) -> Result<Vec<CacheKey>, StoreError> {
        self.search_hashes(hashes).await
    }

    async fn history_stats(&self, filter: &HistoryFilter) -> Result<HistoryStats, StoreError> {
        self.history_stats(filter).await
    }

    async fn suggest(&self, prefix: &str, limit: u32) -> Result<Vec<String>, StoreError> {
        self.suggest(prefix, limit).await
    }

    async fn delete_search_log(&self, id: i64) -> Result<Option<DeleteSearchLog>, StoreError> {
        self.delete_search_log(id).await
    }

    // ---- stats --------------------------------------------------------------

    async fn stats(&self, days: u32) -> Result<StatsSnapshot, StoreError> {
        self.stats(days).await
    }

    // ---- engine health ------------------------------------------------------

    async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
        self.health().await
    }

    async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError> {
        self.put_health(row).await
    }

    // ---- audit --------------------------------------------------------------

    async fn audit(&self, row: AuditRow) -> Result<(), StoreError> {
        self.audit(row).await
    }

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
        self.list_audit(filter).await
    }

    async fn audit_facets(&self) -> Result<AuditFacets, StoreError> {
        self.audit_facets().await
    }
}
