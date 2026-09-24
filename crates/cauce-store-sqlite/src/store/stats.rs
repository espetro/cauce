//! `/api/stats` aggregates over `search_log` in the trailing window plus the
//! live cache/engine panels — one helper per aggregate group.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{
    AdmissionStats, EngineHealthRow, EngineStatsRow, LatencyPercentiles, QueryCount, StatsSnapshot,
    StoreError, TierHit,
};
use rusqlite::{Connection, params};

use crate::rows;

use super::*;

/// Searches, cache hits and deadline hits in the window.
fn totals(conn: &Connection, since: i64) -> Result<(i64, i64, i64), StoreError> {
    conn.query_row(
        "SELECT count(*), coalesce(sum(source = 'cache'), 0),
                coalesce(sum(deadline_hit), 0)
           FROM search_log WHERE ts >= ?1",
        params![since],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .map_err(sql_err)
}

/// p50/p90/p99 over the window's `latency_ms`, `None` on an empty window.
fn latency(conn: &Connection, since: i64) -> Result<Option<LatencyPercentiles>, StoreError> {
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
        Ok(None)
    } else {
        Ok(Some(LatencyPercentiles {
            p50_ms: percentile(&sorted, 50),
            p90_ms: percentile(&sorted, 90),
            p99_ms: percentile(&sorted, 99),
        }))
    }
}

fn by_client(conn: &Connection, since: i64) -> Result<Vec<cauce_core::ClientCount>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT client, count(*) FROM search_log
              WHERE ts >= ?1 GROUP BY client ORDER BY 2 DESC",
        )
        .map_err(sql_err)?;
    stmt.query_map(params![since], |r| {
        Ok(cauce_core::ClientCount {
            client: r.get::<_, String>(0)?,
            searches: r.get::<_, i64>(1)? as u64,
        })
    })
    .map_err(sql_err)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sql_err)
}

fn top_queries(conn: &Connection, since: i64) -> Result<Vec<QueryCount>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT query, count(*) AS n FROM search_log
              WHERE ts >= ?1
              GROUP BY query ORDER BY n DESC, query LIMIT 10",
        )
        .map_err(sql_err)?;
    stmt.query_map(params![since], |r| {
        Ok(QueryCount {
            query: r.get::<_, String>(0)?,
            searches: r.get::<_, i64>(1)? as u64,
        })
    })
    .map_err(sql_err)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sql_err)
}

fn zero_result_queries(conn: &Connection, since: i64) -> Result<Vec<String>, StoreError> {
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
        .map_err(sql_err)
}

/// Cache hits grouped by serving tier (`tier` is only set on
/// `source = 'cache'` rows).
fn hits_by_tier(conn: &Connection, since: i64) -> Result<Vec<TierHit>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT tier, count(*) FROM search_log
              WHERE ts >= ?1 AND source = 'cache' AND tier IS NOT NULL
              GROUP BY tier ORDER BY tier",
        )
        .map_err(sql_err)?;
    stmt.query_map(params![since], |r| {
        Ok(TierHit {
            tier: r.get::<_, i64>(0)? as u8,
            hits: r.get::<_, i64>(1)? as u64,
        })
    })
    .map_err(sql_err)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(sql_err)
}

fn per_day(conn: &Connection, since: i64) -> Result<Vec<cauce_core::DayCount>, StoreError> {
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
            .map(|day| cauce_core::DayCount {
                day,
                searches: searches as u64,
                cache_hits: cache_hits as u64,
            })
            .map_err(|_| StoreError::Corrupt(format!("bad day {d}")))
    })
    .collect::<Result<Vec<_>, _>>()
}

fn engine_rows(conn: &Connection) -> Result<Vec<EngineStatsRow>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT engine_id, ewma_ms, failures, breaker_state,
                    breaker_until, last_ok_at, last_error
               FROM engine_health ORDER BY engine_id",
        )
        .map_err(sql_err)?;
    stmt.query_map([], |r| rows::health(r).map_err(rows::as_sql))
        .map_err(sql_err)?
        .collect::<Result<Vec<EngineHealthRow>, _>>()
        .map_err(sql_err)
        .map(|rows| rows.into_iter().map(EngineStatsRow::from_health).collect())
}

/// Live/expired entry counts plus the W2-03 extras: db file size from the
/// page counters and the newest entry's `created_at` (`max` is NULL on an
/// empty table).
struct CachePanel {
    entries: u64,
    expired: u64,
    db_bytes: u64,
    newest_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn cache_panel(conn: &Connection, now: i64) -> Result<CachePanel, StoreError> {
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
    let cache_db_bytes = {
        let page_count = conn
            .pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))
            .map_err(sql_err)?;
        let page_size = conn
            .pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))
            .map_err(sql_err)?;
        (page_count.max(0) * page_size.max(0)) as u64
    };
    let cache_newest_at = conn
        .query_row("SELECT max(created_at) FROM cache_entries", [], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .map_err(sql_err)?
        .map(rows::from_ms)
        .transpose()?;
    Ok(CachePanel {
        entries: cache_entries as u64,
        expired: cache_entries_expired as u64,
        db_bytes: cache_db_bytes,
        newest_at: cache_newest_at,
    })
}

impl SqliteStore {
    /// All aggregates over `search_log` in the trailing `days` window plus the
    /// live cache/engine panels. `days == 0` means all-time.
    pub(super) async fn stats(&self, days: u32) -> Result<StatsSnapshot, StoreError> {
        self.with_reader(move |conn| {
            let since = if days == 0 {
                i64::MIN
            } else {
                rows::now_ms() - i64::from(days) * 86_400_000
            };
            let now = rows::now_ms();

            let (searches, cache_hits, deadline_hits) = totals(conn, since)?;
            let latency = latency(conn, since)?;
            let by_client = by_client(conn, since)?;
            let top_queries = top_queries(conn, since)?;
            let zero_result_queries = zero_result_queries(conn, since)?;
            let hits_by_tier = hits_by_tier(conn, since)?;
            let per_day = per_day(conn, since)?;
            let engines = engine_rows(conn)?;
            let CachePanel {
                entries: cache_entries,
                expired: cache_entries_expired,
                db_bytes: cache_db_bytes,
                newest_at: cache_newest_at,
            } = cache_panel(conn, now)?;

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
                top_queries,
                zero_result_queries,
                hits_by_tier,
                deadline_hits: deadline_hits as u64,
                per_day,
                engines,
                cache_entries,
                cache_entries_expired,
                cache_db_bytes,
                cache_newest_at,
                // Filled from the in-process metrics registry by the
                // `/api/stats` handler (`StatsSnapshot::merge_metrics`).
                ttfr: None,
                outcomes: Default::default(),
                admission: AdmissionStats::default(),
                // Read from `evals/results/` by the `/api/stats` handler.
                engine_eval: None,
            })
        })
        .await
    }
}
