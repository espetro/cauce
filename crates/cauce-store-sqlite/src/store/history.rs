//! `search_log` and `clicks`: writes, the merged history feed, header stats
//! and single-row deletion.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{
    AnswerLogRow, ClickRow, DeleteAnswerLog, DeleteSearchLog, HistoryFilter, HistoryItem,
    HistoryStats, SearchLogRow, StoreError, normalize_query,
};
use rusqlite::{OptionalExtension, params};

use crate::rows;

use super::*;

impl SqliteStore {
    pub(super) async fn log_search(&self, row: SearchLogRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO search_log
                    (ts, query_hash, query, client, source, tier, latency_ms,
                     result_count, engines_json, deadline_hit, query_raw, origin)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    row.query_raw,
                    row.origin.label(),
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    /// `Store::log_answer` (#254): one durable `answer_log` row per
    /// terminal answer run. No TTL — `evict_expired` never touches it.
    /// Returns the inserted row's id (`/answer/{id}` URLs).
    pub(super) async fn log_answer(&self, row: AnswerLogRow) -> Result<i64, StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO answer_log
                    (ts, query, query_raw, model, answer, confidence,
                     sources_json, related_json, request_id, client, origin,
                     status, ungrounded, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    rows::to_ms(&row.ts),
                    row.query,
                    row.query_raw,
                    row.model,
                    row.answer,
                    row.confidence.map(i64::from),
                    serde_json::to_string(&row.sources)?,
                    serde_json::to_string(&row.related_questions)?,
                    row.request_id.map(|id| id.to_string()),
                    row.client.label(),
                    row.origin.label(),
                    row.status.label(),
                    row.ungrounded,
                    row.error,
                ],
            )
            .map_err(sql_err)?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub(super) async fn record_click(&self, row: ClickRow) -> Result<(), StoreError> {
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

    /// Merged feed: searches and answers honour `q` (substring,
    /// case-insensitive-ish via LIKE), clicks are never filtered by `q`;
    /// `since` applies to all three kinds. `cached=1` (W2-02) keeps only
    /// rows whose `query_hash` has a live `cache_entries` row — one
    /// EXISTS subquery, no join; `answer_log` rows carry no `query_hash`
    /// so `cached` excludes them. `origin` (#254) filters searches and
    /// answers on the stored column, and clicks on the same rule derived
    /// from their client (`ui` is `user`, anything else `agent`).
    pub(super) async fn list_history(
        &self,
        filter: &HistoryFilter,
    ) -> Result<Vec<HistoryItem>, StoreError> {
        let since = filter.since.map(|t| rows::to_ms(&t));
        let q = filter.q.as_deref().map(like_pattern);
        let origin = filter.origin.map(|o| o.label());
        let limit = i64::from(filter.limit);
        let cached = filter.cached;
        self.with_reader(move |conn| {
            let now = rows::now_ms();
            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, query_hash, query, client, source, tier,
                            latency_ms, result_count, engines_json, deadline_hit,
                            query_raw, origin
                       FROM search_log
                      WHERE (?1 IS NULL OR ts >= ?1)
                        AND (?2 IS NULL OR query LIKE ?2 ESCAPE '\\')
                        AND (?3 = 0 OR EXISTS (
                            SELECT 1 FROM cache_entries ce
                             WHERE ce.key = search_log.query_hash
                               AND ce.expires_at > ?4))
                        AND (?6 IS NULL OR origin = ?6)
                      ORDER BY ts DESC, id DESC
                      LIMIT ?5",
                )
                .map_err(sql_err)?;
            let searches = stmt
                .query_map(params![since, q, cached, now, limit, origin], |r| {
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
                        AND (?2 = 0 OR EXISTS (
                            SELECT 1 FROM cache_entries ce
                             WHERE ce.key = clicks.query_hash
                               AND ce.expires_at > ?3))
                        AND (?5 IS NULL OR
                             (CASE WHEN client = 'ui' THEN 'user' ELSE 'agent' END) = ?5)
                      ORDER BY ts DESC, id DESC
                      LIMIT ?4",
                )
                .map_err(sql_err)?;
            let clicks = stmt
                .query_map(params![since, cached, now, limit, origin], |r| {
                    rows::click(r).map_err(rows::as_sql)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;

            // `cached=1` cannot match an `answer_log` row (no
            // `cache_entries` join), so the query is skipped outright.
            let answers = if cached {
                Vec::new()
            } else {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT {}
                           FROM answer_log
                          WHERE (?1 IS NULL OR ts >= ?1)
                            AND (?2 IS NULL OR query LIKE ?2 ESCAPE '\\')
                            AND (?3 IS NULL OR origin = ?3)
                          ORDER BY ts DESC, id DESC
                          LIMIT ?4",
                        rows::ANSWER_LOG_COLS
                    ))
                    .map_err(sql_err)?;
                stmt.query_map(params![since, q, origin, limit], |r| {
                    rows::answer_log(r).map_err(rows::as_sql)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?
            };

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
                .chain(answers.into_iter().map(|a| {
                    (
                        a.ts.timestamp_millis(),
                        a.id.unwrap_or(0),
                        HistoryItem::Answer(a),
                    )
                }))
                .collect();
            items.sort_by_key(|(ts, id, _)| std::cmp::Reverse((*ts, *id)));
            items.truncate(limit.max(0) as usize);
            Ok(items.into_iter().map(|(_, _, item)| item).collect())
        })
        .await
    }

    /// The page's header counts plus `matching`: `search_log` rows satisfying
    /// the same filters as `list_history` (without `limit`).
    pub(super) async fn history_stats(
        &self,
        filter: &HistoryFilter,
    ) -> Result<HistoryStats, StoreError> {
        let since = filter.since.map(|t| rows::to_ms(&t));
        let q = filter.q.as_deref().map(like_pattern);
        let cached = filter.cached;
        let origin = filter.origin.map(|o| o.label());
        self.with_reader(move |conn| {
            let now = rows::now_ms();
            let day_ago = now - 24 * 60 * 60 * 1_000;
            let today_start = chrono::Utc::now()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|t| t.and_utc().timestamp_millis())
                .unwrap_or(now);
            conn.query_row(
                "SELECT
                   (SELECT COUNT(*) FROM search_log WHERE ts >= ?1),
                   (SELECT COUNT(*) FROM search_log),
                   (SELECT COUNT(*) FROM clicks WHERE ts >= ?2),
                   ((SELECT COUNT(*) FROM search_log
                     WHERE (?3 IS NULL OR ts >= ?3)
                       AND (?4 IS NULL OR query LIKE ?4 ESCAPE '\\')
                       AND (?5 = 0 OR EXISTS (
                           SELECT 1 FROM cache_entries ce
                            WHERE ce.key = search_log.query_hash
                              AND ce.expires_at > ?6))
                       AND (?7 IS NULL OR origin = ?7))
                    +
                    (SELECT COUNT(*) FROM answer_log
                     WHERE (?5 = 0)
                       AND (?3 IS NULL OR ts >= ?3)
                       AND (?4 IS NULL OR query LIKE ?4 ESCAPE '\\')
                       AND (?7 IS NULL OR origin = ?7)))",
                params![day_ago, today_start, since, q, cached, now, origin],
                |r| {
                    Ok(HistoryStats {
                        searches_24h: r.get::<_, i64>(0)? as u64,
                        searches_total: r.get::<_, i64>(1)? as u64,
                        clicks_today: r.get::<_, i64>(2)? as u64,
                        matching: r.get::<_, i64>(3)? as u64,
                    })
                },
            )
            .map_err(sql_err)
        })
        .await
    }

    /// `Store::suggest` (#150): distinct normalized queries starting with
    /// `prefix`, ranked by frecency — use count first, most recent use
    /// breaking ties — capped at `limit`. `prefix` goes through
    /// `normalize_query` first, so callers pass user text verbatim and
    /// unicode case/whitespace fold the same way `query` was written;
    /// `LIKE` covers the remaining ASCII case.
    pub(super) async fn suggest(
        &self,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<String>, StoreError> {
        let prefix = normalize_query(prefix);
        if prefix.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let pattern = like_prefix(&prefix);
        let limit = i64::from(limit);
        self.with_reader(move |conn| {
            conn.prepare(
                "SELECT query
                   FROM search_log
                  WHERE query LIKE ?1 ESCAPE '\\'
                  GROUP BY query
                  ORDER BY COUNT(*) DESC, MAX(ts) DESC
                  LIMIT ?2",
            )
            .map_err(sql_err)?
            .query_map(params![pattern, limit], |r| r.get::<_, String>(0))
            .map_err(sql_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err)
        })
        .await
    }

    /// `DELETE /api/history/{id}` (W2-02): one `search_log` row, plus the
    /// `clicks` rows that share its `query_hash` only when no other
    /// `search_log` row carries that hash — a click belongs to the query,
    /// and the surviving newest row still displays them. One transaction,
    /// so the row can never disappear while its clicks stay visible.
    pub(super) async fn delete_search_log(
        &self,
        id: i64,
    ) -> Result<Option<DeleteSearchLog>, StoreError> {
        self.with_writer(move |conn| {
            // `unchecked_transaction` gives a tx from `&Connection`; the
            // writer mutex already guarantees exclusivity.
            let tx = conn.unchecked_transaction().map_err(sql_err)?;
            let found: Option<(String, String)> = tx
                .query_row(
                    "SELECT query_hash, query FROM search_log WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(sql_err)?;
            let Some((query_hash, query)) = found else {
                return Ok(None);
            };
            tx.execute("DELETE FROM search_log WHERE id = ?1", params![id])
                .map_err(sql_err)?;
            let clicks_removed = tx
                .execute(
                    "DELETE FROM clicks WHERE query_hash = ?1
                       AND NOT EXISTS
                         (SELECT 1 FROM search_log WHERE query_hash = ?1)",
                    params![query_hash],
                )
                .map_err(sql_err)? as u64;
            tx.commit().map_err(sql_err)?;
            Ok(Some(DeleteSearchLog {
                query,
                clicks_removed,
            }))
        })
        .await
    }

    /// `GET /answer/{id}` + `GET /api/answer-log/{id}` (#254): one
    /// `answer_log` row by id; `None` when the id is unknown.
    pub(super) async fn get_answer_log(&self, id: i64) -> Result<Option<AnswerLogRow>, StoreError> {
        self.with_reader(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT {} FROM answer_log WHERE id = ?1",
                    rows::ANSWER_LOG_COLS
                ),
                params![id],
                |r| rows::answer_log(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    /// `DELETE /api/answer-log/{id}` (#254): one `answer_log` row; the
    /// query text and status come back for the audit row. Returns `None`
    /// when the id is missing — not an error.
    pub(super) async fn delete_answer_log(
        &self,
        id: i64,
    ) -> Result<Option<DeleteAnswerLog>, StoreError> {
        self.with_writer(move |conn| {
            let found: Option<(String, String)> = conn
                .query_row(
                    "SELECT query, status FROM answer_log WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(sql_err)?;
            let Some((query, status)) = found else {
                return Ok(None);
            };
            conn.execute("DELETE FROM answer_log WHERE id = ?1", params![id])
                .map_err(sql_err)?;
            Ok(Some(DeleteAnswerLog {
                query,
                status: rows::parse_answer_status(&status)?,
            }))
        })
        .await
    }
}
