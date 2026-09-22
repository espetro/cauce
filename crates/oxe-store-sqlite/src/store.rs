//! `SqliteStore`: connection layout, pragmas and the `Store` impl.
//!
//! One writer connection behind a `tokio::sync::Mutex` owns every mutation
//! (WAL allows a single writer); reads go to a small round-robin pool of
//! connections each behind a `std::sync::Mutex`. Every rusqlite call runs in
//! `tokio::task::spawn_blocking` so the async runtime never blocks on disk.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use oxe_core::{
    AuditFilter, AuditRow, CacheKey, CachedSearch, ClickRow, EngineHealthRow, EngineStatus,
    HistoryFilter, HistoryItem, LatencyPercentiles, SearchLogRow, SearchResponse, StatsSnapshot,
    Store, StoreError, StoreTuning,
};
use rusqlite::{Connection, OptionalExtension, params};
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
/// a quoted phrase, so user input can never inject FTS operators.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .filter(|t| t != "\"\"")
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
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

    /// Fresh lookup plus hit counter in one statement on the writer: the
    /// `RETURNING` clause yields the row only when it is unexpired.
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        let key = key.clone();
        self.with_writer(move |conn| {
            conn.query_row(
                &format!(
                    "UPDATE cache_entries SET hits = hits + 1
                      WHERE key = ?1 AND expires_at > ?2
                      RETURNING {}",
                    rows::CACHE_COLS
                ),
                params![key.as_str(), rows::now_ms()],
                |r| rows::cached(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    /// FTS5 over query, titles and snippets, best rank first. Expired rows
    /// are included; the caller inspects `expires_at` to serve or mark them
    /// `stale` (admission contract serves stale rows on overflow).
    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError> {
        let Some(fts) = fts_query(q) else {
            return Ok(Vec::new());
        };
        self.with_reader(move |conn| {
            conn.prepare(&format!(
                "SELECT {} FROM cache_fts
                  JOIN cache_entries ON cache_entries.rowid = cache_fts.rowid
                  WHERE cache_fts MATCH ?1
                  ORDER BY rank
                  LIMIT ?2",
                rows::CACHE_COLS
                    .split(", ")
                    .map(|c| format!("cache_entries.{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .and_then(|mut stmt| {
                stmt.query_map(params![fts, i64::from(limit)], |r| {
                    rows::cached(r).map_err(rows::as_sql)
                })
                .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    /// Upsert. `engines_json` records the engines that produced the stored
    /// response (status `Ok`); `params_json` captures what `put` can see of
    /// the request (its query). FTS sync happens via table triggers.
    async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let key = key.clone();
        let resp = resp.clone();
        self.with_writer(move |conn| {
            let now = rows::now_ms();
            let expires = now.saturating_add(ttl.as_millis().min(i64::MAX as u128) as i64);
            let engines: Vec<_> = resp
                .meta
                .engines_used
                .iter()
                .filter(|r| matches!(r.status, EngineStatus::Ok))
                .map(|r| r.engine.clone())
                .collect();
            conn.execute(
                "INSERT INTO cache_entries
                    (key, query, params_json, payload_json, created_at, expires_at, hits, engines_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
                 ON CONFLICT(key) DO UPDATE SET
                    query = excluded.query,
                    params_json = excluded.params_json,
                    payload_json = excluded.payload_json,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at,
                    hits = 0,
                    engines_json = excluded.engines_json",
                params![
                    key.as_str(),
                    resp.query,
                    serde_json::to_string(&serde_json::json!({ "q": resp.query }))?,
                    serde_json::to_string(&resp)?,
                    now,
                    expires,
                    rows::engines_to_json(&engines)?,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    async fn evict_expired(&self) -> Result<u64, StoreError> {
        self.with_writer(|conn| {
            conn.execute(
                "DELETE FROM cache_entries WHERE expires_at <= ?1",
                params![rows::now_ms()],
            )
            .map(|n| n as u64)
            .map_err(sql_err)
        })
        .await
    }

    // ---- cache admin --------------------------------------------------------

    /// Newest first; includes expired-but-not-yet-evicted rows.
    async fn list_cache(&self, limit: u32, offset: u32) -> Result<Vec<CachedSearch>, StoreError> {
        self.with_reader(move |conn| {
            conn.prepare(&format!(
                "SELECT {} FROM cache_entries
                  ORDER BY created_at DESC, rowid DESC
                  LIMIT ?1 OFFSET ?2",
                rows::CACHE_COLS
            ))
            .and_then(|mut stmt| {
                stmt.query_map(params![i64::from(limit), i64::from(offset)], |r| {
                    rows::cached(r).map_err(rows::as_sql)
                })
                .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        let key = key.clone();
        self.with_reader(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT {} FROM cache_entries WHERE key = ?1",
                    rows::CACHE_COLS
                ),
                params![key.as_str()],
                |r| rows::cached(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError> {
        let key = key.clone();
        self.with_writer(move |conn| {
            conn.execute(
                "DELETE FROM cache_entries WHERE key = ?1",
                params![key.as_str()],
            )
            .map(|n| n > 0)
            .map_err(sql_err)
        })
        .await
    }

    async fn clear_cache(&self) -> Result<u64, StoreError> {
        self.with_writer(|conn| {
            conn.execute("DELETE FROM cache_entries", [])
                .map(|n| n as u64)
                .map_err(sql_err)
        })
        .await
    }

    // ---- search log, clicks, history ----------------------------------------

    async fn log_search(&self, row: SearchLogRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO search_log
                    (ts, query_hash, query, client, source, tier, latency_ms,
                     result_count, engines_json, deadline_hit)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    rows::to_ms(&row.ts),
                    row.query_hash.as_str(),
                    row.query,
                    row.client.label(),
                    rows::source_str(row.source),
                    row.tier.map(|t| i64::from(t.as_u8())),
                    i64::from(row.latency_ms),
                    i64::from(row.result_count),
                    rows::engines_to_json(&row.engines)?,
                    row.deadline_hit,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    async fn record_click(&self, row: ClickRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO clicks (ts, query_hash, url, title, position, client)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rows::to_ms(&row.ts),
                    row.query_hash.map(|k| k.as_str().to_string()),
                    row.url.as_str(),
                    row.title,
                    i64::from(row.position),
                    row.client.label(),
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    /// Merged feed: searches honour `q` (substring, case-insensitive-ish via
    /// LIKE), clicks are never filtered by `q`; `since` applies to both.
    async fn list_history(&self, filter: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError> {
        let since = filter.since.map(|t| rows::to_ms(&t));
        let q = filter.q.as_deref().map(like_pattern);
        let limit = i64::from(filter.limit);
        self.with_reader(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, query_hash, query, client, source, tier,
                            latency_ms, result_count, engines_json, deadline_hit
                       FROM search_log
                      WHERE (?1 IS NULL OR ts >= ?1)
                        AND (?2 IS NULL OR query LIKE ?2 ESCAPE '\\')
                      ORDER BY ts DESC, id DESC
                      LIMIT ?3",
                )
                .map_err(sql_err)?;
            let searches = stmt
                .query_map(params![since, q, limit], |r| {
                    rows::search_log(r).map_err(rows::as_sql)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, query_hash, url, title, position, client
                       FROM clicks
                      WHERE (?1 IS NULL OR ts >= ?1)
                      ORDER BY ts DESC, id DESC
                      LIMIT ?2",
                )
                .map_err(sql_err)?;
            let clicks = stmt
                .query_map(params![since, limit], |r| {
                    rows::click(r).map_err(rows::as_sql)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;

            // Merge newest-first by (ts, id) and cap at the requested limit.
            let mut items: Vec<(i64, i64, HistoryItem)> = searches
                .into_iter()
                .map(|s| {
                    (
                        s.ts.timestamp_millis(),
                        s.id.unwrap_or(0),
                        HistoryItem::Search(s),
                    )
                })
                .chain(clicks.into_iter().map(|c| {
                    (
                        c.ts.timestamp_millis(),
                        c.id.unwrap_or(0),
                        HistoryItem::Click(c),
                    )
                }))
                .collect();
            items.sort_by_key(|(ts, id, _)| std::cmp::Reverse((*ts, *id)));
            items.truncate(limit.max(0) as usize);
            Ok(items.into_iter().map(|(_, _, item)| item).collect())
        })
        .await
    }

    // ---- stats --------------------------------------------------------------

    /// All aggregates over `search_log` in the trailing `days` window plus the
    /// live cache/engine panels. `days == 0` means all-time.
    async fn stats(&self, days: u32) -> Result<StatsSnapshot, StoreError> {
        self.with_reader(move |conn| {
            let since = if days == 0 {
                i64::MIN
            } else {
                rows::now_ms() - i64::from(days) * 86_400_000
            };
            let now = rows::now_ms();

            let (searches, cache_hits): (i64, i64) = conn
                .query_row(
                    "SELECT count(*), coalesce(sum(source = 'cache'), 0)
                       FROM search_log WHERE ts >= ?1",
                    params![since],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(sql_err)?;

            let latency = {
                let mut stmt = conn
                    .prepare(
                        "SELECT latency_ms FROM search_log
                          WHERE ts >= ?1 ORDER BY latency_ms",
                    )
                    .map_err(sql_err)?;
                let mut sorted: Vec<u32> = stmt
                    .query_map(params![since], |r| r.get::<_, u32>(0))
                    .map_err(sql_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_err)?;
                sorted.sort_unstable();
                if sorted.is_empty() {
                    None
                } else {
                    Some(LatencyPercentiles {
                        p50_ms: percentile(&sorted, 50),
                        p90_ms: percentile(&sorted, 90),
                        p99_ms: percentile(&sorted, 99),
                    })
                }
            };

            let by_client = {
                let mut stmt = conn
                    .prepare(
                        "SELECT client, count(*) FROM search_log
                          WHERE ts >= ?1 GROUP BY client ORDER BY 2 DESC",
                    )
                    .map_err(sql_err)?;
                stmt.query_map(params![since], |r| {
                    Ok(oxe_core::ClientCount {
                        client: r.get::<_, String>(0)?,
                        searches: r.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?
            };

            let zero_result_queries = {
                let mut stmt = conn
                    .prepare(
                        "SELECT query, count(*) AS n FROM search_log
                          WHERE ts >= ?1 AND result_count = 0
                          GROUP BY query ORDER BY n DESC, query LIMIT 10",
                    )
                    .map_err(sql_err)?;
                stmt.query_map(params![since], |r| r.get::<_, String>(0))
                    .map_err(sql_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_err)?
            };

            let per_day = {
                let mut stmt = conn
                    .prepare(
                        "SELECT date(ts / 1000, 'unixepoch') AS d,
                                count(*), coalesce(sum(source = 'cache'), 0)
                           FROM search_log WHERE ts >= ?1
                          GROUP BY d ORDER BY d",
                    )
                    .map_err(sql_err)?;
                stmt.query_map(params![since], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?
                .into_iter()
                .map(|(d, searches, cache_hits)| {
                    chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                        .map(|day| oxe_core::DayCount {
                            day,
                            searches: searches as u64,
                            cache_hits: cache_hits as u64,
                        })
                        .map_err(|_| StoreError::Corrupt(format!("bad day {d}")))
                })
                .collect::<Result<Vec<_>, _>>()?
            };

            let engines = {
                let mut stmt = conn
                    .prepare(
                        "SELECT engine_id, ewma_ms, failures, breaker_state,
                                breaker_until, last_ok_at, last_error
                           FROM engine_health ORDER BY engine_id",
                    )
                    .map_err(sql_err)?;
                stmt.query_map([], |r| rows::health(r).map_err(rows::as_sql))
                    .map_err(sql_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_err)?
            };

            let cache_entries: i64 = conn
                .query_row(
                    "SELECT count(*) FROM cache_entries WHERE expires_at > ?1",
                    params![now],
                    |r| r.get(0),
                )
                .map_err(sql_err)?;
            let cache_entries_expired: i64 = conn
                .query_row(
                    "SELECT count(*) FROM cache_entries WHERE expires_at <= ?1",
                    params![now],
                    |r| r.get(0),
                )
                .map_err(sql_err)?;

            let searches = searches as u64;
            let cache_hits = cache_hits as u64;
            Ok(StatsSnapshot {
                window_days: days,
                searches,
                cache_hits,
                hit_rate: if searches == 0 {
                    0.0
                } else {
                    cache_hits as f64 / searches as f64
                },
                latency,
                by_client,
                zero_result_queries,
                per_day,
                engines,
                cache_entries: cache_entries as u64,
                cache_entries_expired: cache_entries_expired as u64,
            })
        })
        .await
    }

    // ---- engine health ------------------------------------------------------

    async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
        self.with_reader(|conn| {
            conn.prepare(
                "SELECT engine_id, ewma_ms, failures, breaker_state,
                        breaker_until, last_ok_at, last_error
                   FROM engine_health ORDER BY engine_id",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| rows::health(r).map_err(rows::as_sql))
                    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError> {
        let row = row.clone();
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO engine_health
                    (engine_id, ewma_ms, failures, breaker_state,
                     breaker_until, last_ok_at, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(engine_id) DO UPDATE SET
                    ewma_ms = excluded.ewma_ms,
                    failures = excluded.failures,
                    breaker_state = excluded.breaker_state,
                    breaker_until = excluded.breaker_until,
                    last_ok_at = excluded.last_ok_at,
                    last_error = excluded.last_error",
                params![
                    row.engine.as_str(),
                    row.ewma_ms,
                    i64::from(row.failures),
                    rows::breaker_str(row.breaker),
                    row.breaker_until.map(|t| rows::to_ms(&t)),
                    row.last_ok_at.map(|t| rows::to_ms(&t)),
                    row.last_error,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    // ---- audit --------------------------------------------------------------

    async fn audit(&self, row: AuditRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO audit (ts, actor, action, target, details_json, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rows::to_ms(&row.ts),
                    row.actor,
                    row.action,
                    row.target,
                    serde_json::to_string(&row.details)?,
                    row.request_id.map(|u| u.to_string()),
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
        let since = filter.since.map(|t| rows::to_ms(&t));
        let actor = filter.actor.clone();
        let action = filter.action.clone();
        let limit = i64::from(filter.limit);
        self.with_reader(move |conn| {
            conn.prepare(
                "SELECT id, ts, actor, action, target, details_json, request_id
                   FROM audit
                  WHERE (?1 IS NULL OR ts >= ?1)
                    AND (?2 IS NULL OR actor = ?2)
                    AND (?3 IS NULL OR action = ?3)
                  ORDER BY ts DESC, id DESC
                  LIMIT ?4",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![since, actor, action, limit], |r| {
                    rows::audit(r).map_err(rows::as_sql)
                })
                .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }
}
