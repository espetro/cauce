//! `cache_entries`: tier-1 exact get, FTS5 lexical get, upsert and eviction,
//! the admin CRUD, and the batched `cache_states`/`search_hashes` lookups the
//! history page joins against.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::time::Duration;

use cauce_core::{
    CacheKey, CacheResultHit, CacheState, CachedSearch, EngineStatus, SearchResponse, SearchResult,
    StoreError,
};
use rusqlite::{OptionalExtension, params};

use crate::rows;

use super::*;

impl SqliteStore {
    /// Fresh lookup plus hit counter in one statement on the writer: the
    /// `RETURNING` clause yields the row only when it is unexpired.
    pub(super) async fn get_exact(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachedSearch>, StoreError> {
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
    pub(super) async fn get_lexical(
        &self,
        q: &str,
        limit: u8,
    ) -> Result<Vec<CachedSearch>, StoreError> {
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

    /// `search_archive`'s `cached_result` arm (W5-03): `cache_fts` MATCH
    /// picks candidate entries (bm25 order, expired rows included) and
    /// each contributes the stored results whose title+snippet tokens
    /// cover every query term — `titles`/`snippets` are index-only
    /// columns, so the per-result filter runs on the decoded
    /// `payload_json`. A query-column-only match contributes nothing;
    /// the output may be shorter than `limit` when filtering drops hits.
    pub(super) async fn search_cache_fts(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<CacheResultHit>, StoreError> {
        let Some(fts) = fts_query(q) else {
            return Ok(Vec::new());
        };
        let terms = cache_fts_query_terms(q);
        let want = usize::try_from(limit).unwrap_or(usize::MAX);
        self.with_reader(move |conn| {
            let cols = rows::CACHE_COLS
                .split(", ")
                .map(|c| format!("cache_entries.{c}"))
                .collect::<Vec<_>>()
                .join(", ");
            conn.prepare(&format!(
                "SELECT {cols}, bm25(cache_fts) AS rank FROM cache_fts
                  JOIN cache_entries ON cache_entries.rowid = cache_fts.rowid
                  WHERE cache_fts MATCH ?1
                  ORDER BY rank
                  LIMIT ?2"
            ))
            .and_then(|mut stmt| {
                stmt.query_map(params![fts, i64::from(limit)], |r| {
                    let entry = rows::cached(r).map_err(rows::as_sql)?;
                    let rank: f64 = r
                        .get(rows::CACHE_COLS.split(", ").count())
                        .map_err(|e| rows::as_sql(StoreError::Corrupt(format!("rank: {e}"))))?;
                    Ok((entry, rank))
                })
                .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map(|entries| {
                let mut hits = Vec::new();
                'entries: for (entry, rank) in entries {
                    for res in &entry.response.results {
                        if hits.len() >= want {
                            break 'entries;
                        }
                        if cache_result_covers(&terms, res) {
                            hits.push(CacheResultHit {
                                url: res.url.clone(),
                                title: res.title.clone(),
                                snippet: res.snippet.clone(),
                                engine: res.engine.clone(),
                                query: entry.query.clone(),
                                expires_at: entry.expires_at,
                                score: Some(rank),
                            });
                        }
                    }
                }
                hits
            })
            .map_err(sql_err)
        })
        .await
    }

    /// Upsert. `engines_json` records the engines that produced the stored
    /// response (status `Ok`); `params_json` captures what `put` can see of
    /// the request (its query). FTS sync happens via table triggers.
    pub(super) async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let key = key.clone();
        let resp = resp.clone();
        self.with_writer(move |conn| {
            let now = rows::now_ms();
            let expires = now
                .saturating_add(ttl.as_millis().min(i64::MAX as u128) as i64)
                .min(max_ts_ms());
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

    /// Rows are only collected once they are `grace` past expiry: inside
    /// the W3-02 stale-serve window an expired row can still answer.
    pub(super) async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError> {
        let cutoff =
            rows::now_ms().saturating_sub(i64::try_from(grace.as_millis()).unwrap_or(i64::MAX));
        self.with_writer(move |conn| {
            conn.execute(
                "DELETE FROM cache_entries WHERE expires_at <= ?1",
                params![cutoff],
            )
            .map(|n| n as u64)
            .map_err(sql_err)
        })
        .await
    }

    /// Newest first; includes expired-but-not-yet-evicted rows.
    pub(super) async fn list_cache(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CachedSearch>, StoreError> {
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

    pub(super) async fn get_cache(
        &self,
        key: &CacheKey,
    ) -> Result<Option<CachedSearch>, StoreError> {
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

    pub(super) async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError> {
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

    pub(super) async fn clear_cache(&self) -> Result<u64, StoreError> {
        self.with_writer(|conn| {
            conn.execute("DELETE FROM cache_entries", [])
                .map(|n| n as u64)
                .map_err(sql_err)
        })
        .await
    }

    /// Batched `cache_entries` lookup for the history `source` column: one
    /// `IN` query for the page's distinct `query_hash`es, expired rows
    /// included (the page renders `cached · expired` for them).
    pub(super) async fn cache_states(
        &self,
        keys: &[CacheKey],
    ) -> Result<Vec<CacheState>, StoreError> {
        let keys: Vec<String> = keys.iter().map(|k| k.as_str().to_string()).collect();
        self.with_reader(move |conn| {
            if keys.is_empty() {
                return Ok(Vec::new());
            }
            let marks = std::iter::repeat_n("?", keys.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT key, query, created_at, expires_at
                   FROM cache_entries WHERE key IN ({marks})"
            );
            let mut stmt = conn.prepare(&sql).map_err(sql_err)?;
            let rows_out = stmt
                .query_map(rusqlite::params_from_iter(keys.iter()), |r| {
                    let key: String = r
                        .get(0)
                        .map_err(|e| rows::as_sql(StoreError::Corrupt(format!("key: {e}"))))?;
                    Ok(CacheState {
                        key: key
                            .parse()
                            .map_err(|e: String| rows::as_sql(StoreError::Corrupt(e)))?,
                        query: r.get(1).map_err(|e| {
                            rows::as_sql(StoreError::Corrupt(format!("query: {e}")))
                        })?,
                        created_at: rows::from_ms(r.get(2).map_err(|e| {
                            rows::as_sql(StoreError::Corrupt(format!("created_at: {e}")))
                        })?)
                        .map_err(rows::as_sql)?,
                        expires_at: rows::from_ms(r.get(3).map_err(|e| {
                            rows::as_sql(StoreError::Corrupt(format!("expires_at: {e}")))
                        })?)
                        .map_err(rows::as_sql)?,
                    })
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;
            Ok(rows_out)
        })
        .await
    }

    /// Batched `search_log` existence check for the history page's orphan
    /// clicks: one `IN` query returning the subset of `hashes` that still
    /// have a `search_log` row.
    pub(super) async fn search_hashes(
        &self,
        hashes: &[CacheKey],
    ) -> Result<Vec<CacheKey>, StoreError> {
        let hashes: Vec<String> = hashes.iter().map(|k| k.as_str().to_string()).collect();
        self.with_reader(move |conn| {
            if hashes.is_empty() {
                return Ok(Vec::new());
            }
            let marks = std::iter::repeat_n("?", hashes.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql =
                format!("SELECT DISTINCT query_hash FROM search_log WHERE query_hash IN ({marks})");
            let mut stmt = conn.prepare(&sql).map_err(sql_err)?;
            let rows_out = stmt
                .query_map(rusqlite::params_from_iter(hashes.iter()), |r| {
                    r.get::<_, String>(0)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;
            rows_out
                .into_iter()
                .map(|h| h.parse().map_err(|e: String| StoreError::Corrupt(e)))
                .collect()
        })
        .await
    }
}

/// Alphanumeric token set of `text`, lowercased — the Rust mirror of
/// FTS5's unicode61 tokenizer (split on non-alphanumerics), used for the
/// result-level cover check the index-only `titles`/`snippets` columns
/// cannot answer.
fn cache_fts_tokens(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The query's token set: whitespace terms → their unicode61-equivalent
/// tokens, dropping terms with no alphanumeric character (the same rule
/// `fts_query` applies before quoting them).
fn cache_fts_query_terms(q: &str) -> HashSet<String> {
    q.split_whitespace()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .flat_map(cache_fts_tokens)
        .collect()
}

/// The `search_cache_fts` per-result cover check: every query token
/// appears in the result's title+snippet token set — the set-cover
/// approximation of the FTS5 phrase match the entry already passed.
fn cache_result_covers(terms: &HashSet<String>, res: &SearchResult) -> bool {
    let hay = cache_fts_tokens(&format!("{} {}", res.title, res.snippet));
    terms.iter().all(|t| hay.contains(t))
}
