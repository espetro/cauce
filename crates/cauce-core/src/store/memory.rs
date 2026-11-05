//! In-memory ephemeral implementation of `Store`.
//!
//! Provides a zero-dependency, lock-based (standard library `RwLock`) storage engine
//! suitable for WASM components, Cloudflare Workers, AWS Lambda, tests, and ephemeral setups.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::RwLock;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use url::Url;
use uuid::Uuid;

use crate::cache::{CacheKey, CachedSearch, normalize_query};
use crate::engine::EngineId;
use crate::request::ClientKind;
use crate::response::SearchResponse;
use crate::store::{
    AdmissionStats, AnswerKey, AnswerLogRow, AnswerRow, AnswerThreadRow, AuditFacets, AuditFilter,
    AuditRow, CacheResultHit, CacheState, CachedAnswer, ClickRow, ClientCount, DayCount,
    DeleteAnswerLog, DeleteSearchLog, EngineHealthRow, EngineStatsRow, HistoryFilter, HistoryItem,
    HistoryStats, LatencyPercentiles, LogSource, PAGE_MARK_CLOSE, PAGE_MARK_OPEN, PageHit, PageRow,
    QueryCount, SearchLogRow, StatsSnapshot, Store, StoreError, TierHit,
};

#[derive(Default)]
struct Inner {
    // Cache entries: keyed by CacheKey
    cache: HashMap<CacheKey, CachedSearch>,

    // Answers: keyed by AnswerKey
    answers: HashMap<AnswerKey, CachedAnswer>,

    // Pages: keyed by Url
    pages: HashMap<Url, PageRow>,

    // Search log: auto-incrementing id -> SearchLogRow
    search_log_seq: i64,
    search_log: BTreeMap<i64, SearchLogRow>,

    // Answer log: auto-incrementing id -> AnswerLogRow
    answer_log_seq: i64,
    answer_log: BTreeMap<i64, AnswerLogRow>,

    // Clicks: auto-incrementing id -> ClickRow
    click_seq: i64,
    clicks: BTreeMap<i64, ClickRow>,

    // Answer threads: keyed by thread Uuid
    threads: HashMap<Uuid, AnswerThreadRow>,

    // Engine health: keyed by EngineId
    health: HashMap<EngineId, EngineHealthRow>,

    // Audit logs: auto-incrementing id -> AuditRow
    audit_seq: i64,
    audit: BTreeMap<i64, AuditRow>,
}

/// An in-memory store implementing [`Store`].
///
/// Thread-safe and async-safe via `std::sync::RwLock`.
#[derive(Default)]
pub struct MemoryStore {
    inner: RwLock<Inner>,
}

impl MemoryStore {
    /// Create a new, empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

fn tokenize(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

fn query_terms(q: &str) -> HashSet<String> {
    tokenize(q)
}

fn percentile(sorted: &[u32], p: u64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as u64 * p + 99) / 100).saturating_sub(1) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn calculate_expiry(now: DateTime<Utc>, ttl: Duration) -> DateTime<Utc> {
    let now_ms = now.timestamp_millis();
    let max_ms = DateTime::<Utc>::MAX_UTC.timestamp_millis();
    let add_ms = ttl.as_millis().min(i64::MAX as u128) as i64;
    let exp_ms = now_ms.saturating_add(add_ms).min(max_ms);
    DateTime::from_timestamp_millis(exp_ms).unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn calculate_cutoff(now: DateTime<Utc>, grace: Duration) -> DateTime<Utc> {
    let now_ms = now.timestamp_millis();
    let sub_ms = grace.as_millis().min(i64::MAX as u128) as i64;
    let cut_ms = now_ms.saturating_sub(sub_ms);
    DateTime::from_timestamp_millis(cut_ms).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

#[async_trait]
impl Store for MemoryStore {
    // ---- tier-1 exact cache -------------------------------------------------

    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        let mut inner = self.inner.write().unwrap();
        let now = Utc::now();
        if let Some(entry) = inner.cache.get_mut(key) {
            if entry.expires_at > now {
                entry.hits += 1;
                return Ok(Some(entry.clone()));
            }
        }
        Ok(None)
    }

    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError> {
        let terms = query_terms(q);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.inner.read().unwrap();
        let mut matches = Vec::new();

        for entry in inner.cache.values() {
            // Index over stored query, result titles and snippets
            let mut text = entry.query.clone();
            for res in &entry.response.results {
                text.push(' ');
                text.push_str(&res.title);
                text.push(' ');
                text.push_str(&res.snippet);
            }
            let tokens = tokenize(&text);
            if terms.iter().all(|t| tokens.contains(t)) {
                matches.push(entry.clone());
            }
        }

        // Newest created first
        matches.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        matches.truncate(limit as usize);
        Ok(matches)
    }

    async fn search_cache_fts(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<CacheResultHit>, StoreError> {
        let terms = query_terms(q);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.inner.read().unwrap();
        let mut hits = Vec::new();

        for entry in inner.cache.values() {
            for res in &entry.response.results {
                let mut text = res.title.clone();
                text.push(' ');
                text.push_str(&res.snippet);
                let tokens = tokenize(&text);
                if terms.iter().all(|t| tokens.contains(t)) {
                    hits.push(CacheResultHit {
                        url: res.url.clone(),
                        title: res.title.clone(),
                        snippet: res.snippet.clone(),
                        engine: res.engine.clone(),
                        query: entry.query.clone(),
                        expires_at: entry.expires_at,
                        score: None,
                    });
                }
            }
        }

        hits.sort_by(|a, b| b.expires_at.cmp(&a.expires_at));
        hits.truncate(limit as usize);
        Ok(hits)
    }

    async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let now = Utc::now();
        let expires_at = calculate_expiry(now, ttl);
        let engines = resp
            .meta
            .engines_used
            .iter()
            .map(|r| r.engine.clone())
            .collect();

        let entry = CachedSearch {
            key: key.clone(),
            query: resp.query.clone(),
            params: serde_json::json!({ "q": resp.query }),
            response: resp.clone(),
            created_at: now,
            expires_at,
            hits: 0,
            engines,
        };

        let mut inner = self.inner.write().unwrap();
        inner.cache.insert(key.clone(), entry);
        Ok(())
    }

    async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError> {
        let now = Utc::now();
        let cutoff = calculate_cutoff(now, grace);

        let mut inner = self.inner.write().unwrap();
        let before = inner.cache.len();
        inner.cache.retain(|_, v| v.expires_at > cutoff);
        let removed = before - inner.cache.len();
        Ok(removed as u64)
    }

    // ---- cache admin --------------------------------------------------------

    async fn list_cache(&self, limit: u32, offset: u32) -> Result<Vec<CachedSearch>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut entries: Vec<CachedSearch> = inner.cache.values().cloned().collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let res = entries
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok(res)
    }

    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        let inner = self.inner.read().unwrap();
        Ok(inner.cache.get(key).cloned())
    }

    async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError> {
        let mut inner = self.inner.write().unwrap();
        Ok(inner.cache.remove(key).is_some())
    }

    async fn clear_cache(&self) -> Result<u64, StoreError> {
        let mut inner = self.inner.write().unwrap();
        let count = inner.cache.len() as u64;
        inner.cache.clear();
        Ok(count)
    }

    // ---- answers ------------------------------------------------------------

    async fn get_answer(&self, key: &AnswerKey) -> Result<Option<CachedAnswer>, StoreError> {
        let inner = self.inner.read().unwrap();
        let now = Utc::now();
        if let Some(ans) = inner.answers.get(key) {
            if ans.expires_at > now {
                return Ok(Some(ans.clone()));
            }
        }
        Ok(None)
    }

    async fn put_answer(
        &self,
        key: &AnswerKey,
        row: &AnswerRow,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let now = Utc::now();
        let expires_at = calculate_expiry(now, ttl);

        let answer = CachedAnswer {
            key: key.clone(),
            query: row.query.clone(),
            model: row.model.clone(),
            payload: row.payload.clone(),
            sources: row.sources.clone(),
            created_at: now,
            expires_at,
        };

        let mut inner = self.inner.write().unwrap();
        inner.answers.insert(key.clone(), answer);
        Ok(())
    }

    // ---- pages --------------------------------------------------------------

    async fn put_page(&self, row: &PageRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.pages.insert(row.url.clone(), row.clone());
        Ok(())
    }

    async fn get_page(&self, url: &Url) -> Result<Option<PageRow>, StoreError> {
        let inner = self.inner.read().unwrap();
        Ok(inner.pages.get(url).cloned())
    }

    async fn search_pages(&self, q: &str, limit: u32) -> Result<Vec<PageHit>, StoreError> {
        let terms = query_terms(q);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let inner = self.inner.read().unwrap();
        let mut hits = Vec::new();

        for page in inner.pages.values() {
            let mut text = page.title.clone();
            text.push(' ');
            text.push_str(&page.markdown);
            let tokens = tokenize(&text);
            if terms.iter().all(|t| tokens.contains(t)) {
                // Generate marked snippet
                let snippet = if let Some(first_term) = terms.iter().next() {
                    if let Some(pos) = page.markdown.to_lowercase().find(first_term) {
                        let start = pos.saturating_sub(40);
                        let end = (pos + first_term.len() + 40).min(page.markdown.len());
                        let matched = &page.markdown[pos..pos + first_term.len()];
                        format!(
                            "…{}{PAGE_MARK_OPEN}{matched}{PAGE_MARK_CLOSE}{}…",
                            &page.markdown[start..pos],
                            &page.markdown[pos + first_term.len()..end]
                        )
                    } else {
                        page.markdown.chars().take(240).collect()
                    }
                } else {
                    page.markdown.chars().take(240).collect()
                };

                hits.push(PageHit {
                    url: page.url.clone(),
                    title: page.title.clone(),
                    snippet,
                    fetched_at: page.fetched_at,
                    score: Some(-1.0),
                });
            }
        }

        hits.sort_by(|a, b| b.fetched_at.cmp(&a.fetched_at));
        hits.truncate(limit as usize);
        Ok(hits)
    }

    async fn list_pages(&self, limit: u32, offset: u32) -> Result<Vec<PageHit>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut pages: Vec<&PageRow> = inner.pages.values().collect();
        pages.sort_by(|a, b| b.fetched_at.cmp(&a.fetched_at));

        let res = pages
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|p| PageHit {
                url: p.url.clone(),
                title: p.title.clone(),
                snippet: p.markdown.chars().take(240).collect(),
                fetched_at: p.fetched_at,
                score: None,
            })
            .collect();
        Ok(res)
    }

    async fn delete_page(&self, url: &Url) -> Result<bool, StoreError> {
        let mut inner = self.inner.write().unwrap();
        Ok(inner.pages.remove(url).is_some())
    }

    // ---- search log, clicks, history ----------------------------------------

    async fn log_search(&self, mut row: SearchLogRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.search_log_seq += 1;
        let id = inner.search_log_seq;
        row.id = Some(id);
        inner.search_log.insert(id, row);
        Ok(())
    }

    async fn log_answer(&self, mut row: AnswerLogRow) -> Result<i64, StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.answer_log_seq += 1;
        row.id = Some(inner.answer_log_seq);
        let id = inner.answer_log_seq;
        inner.answer_log.insert(id, row);
        Ok(id)
    }

    async fn get_answer_log(&self, id: i64) -> Result<Option<AnswerLogRow>, StoreError> {
        let inner = self.inner.read().unwrap();
        Ok(inner.answer_log.get(&id).cloned())
    }

    async fn delete_answer_log(&self, id: i64) -> Result<Option<DeleteAnswerLog>, StoreError> {
        let mut inner = self.inner.write().unwrap();
        if let Some(row) = inner.answer_log.remove(&id) {
            Ok(Some(DeleteAnswerLog {
                query: row.query,
                status: row.status,
            }))
        } else {
            Ok(None)
        }
    }

    async fn record_click(&self, mut row: ClickRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.click_seq += 1;
        let id = inner.click_seq;
        row.id = Some(id);
        inner.clicks.insert(id, row);
        Ok(())
    }

    // ---- answer threads -----------------------------------------------------

    async fn upsert_thread(&self, row: AnswerThreadRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.threads.insert(row.id, row);
        Ok(())
    }

    async fn list_threads(
        &self,
        owner: &str,
        limit: u32,
    ) -> Result<Vec<AnswerThreadRow>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut threads: Vec<AnswerThreadRow> = inner
            .threads
            .values()
            .filter(|t| t.owner == owner)
            .cloned()
            .collect();
        threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        threads.truncate(limit as usize);
        Ok(threads)
    }

    async fn get_thread(
        &self,
        id: &str,
        owner: &str,
    ) -> Result<Option<AnswerThreadRow>, StoreError> {
        let uuid = match Uuid::parse_str(id) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let inner = self.inner.read().unwrap();
        if let Some(t) = inner.threads.get(&uuid) {
            if t.owner == owner {
                return Ok(Some(t.clone()));
            }
        }
        Ok(None)
    }

    async fn delete_thread(&self, id: &str, owner: &str) -> Result<bool, StoreError> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| StoreError::Corrupt(format!("thread id {id:?}: {e}")))?;
        let mut inner = self.inner.write().unwrap();
        if let Some(t) = inner.threads.get(&uuid) {
            if t.owner == owner {
                inner.threads.remove(&uuid);
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ---- history ------------------------------------------------------------

    async fn list_history(&self, filter: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError> {
        let inner = self.inner.read().unwrap();
        let now = Utc::now();
        let mut items = Vec::new();

        // Searches
        for s in inner.search_log.values() {
            if let Some(since) = filter.since {
                if s.ts < since {
                    continue;
                }
            }
            if let Some(q) = &filter.q {
                if !s.query.contains(q) {
                    continue;
                }
            }
            if let Some(origin) = filter.origin {
                if s.origin != origin {
                    continue;
                }
            }
            if filter.cached {
                match inner.cache.get(&s.query_hash) {
                    Some(entry) if entry.expires_at > now => {}
                    _ => continue,
                }
            }
            items.push((s.ts, s.id.unwrap_or(0), HistoryItem::Search(s.clone())));
        }

        // Clicks
        for c in inner.clicks.values() {
            if let Some(since) = filter.since {
                if c.ts < since {
                    continue;
                }
            }
            // `q` filter does not apply to clicks
            if let Some(origin) = filter.origin {
                let click_origin = match c.client {
                    ClientKind::Ui => crate::SearchOrigin::User,
                    _ => crate::SearchOrigin::Agent,
                };
                if click_origin != origin {
                    continue;
                }
            }
            if filter.cached {
                if let Some(hash) = &c.query_hash {
                    match inner.cache.get(hash) {
                        Some(entry) if entry.expires_at > now => {}
                        _ => continue,
                    }
                } else {
                    continue;
                }
            }
            items.push((c.ts, c.id.unwrap_or(0), HistoryItem::Click(c.clone())));
        }

        // Answers
        if !filter.cached {
            for a in inner.answer_log.values() {
                if let Some(since) = filter.since {
                    if a.ts < since {
                        continue;
                    }
                }
                if let Some(q) = &filter.q {
                    if !a.query.contains(q) {
                        continue;
                    }
                }
                if let Some(origin) = filter.origin {
                    if a.origin != origin {
                        continue;
                    }
                }
                items.push((a.ts, a.id.unwrap_or(0), HistoryItem::Answer(a.clone())));
            }
        }

        // Order newest first (ts desc, id desc)
        items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        items.truncate(filter.limit as usize);

        Ok(items.into_iter().map(|(_, _, item)| item).collect())
    }

    async fn cache_states(&self, keys: &[CacheKey]) -> Result<Vec<CacheState>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut states = Vec::new();
        for k in keys {
            if let Some(entry) = inner.cache.get(k) {
                states.push(CacheState {
                    key: entry.key.clone(),
                    query: entry.query.clone(),
                    created_at: entry.created_at,
                    expires_at: entry.expires_at,
                });
            }
        }
        Ok(states)
    }

    async fn search_hashes(&self, hashes: &[CacheKey]) -> Result<Vec<CacheKey>, StoreError> {
        let inner = self.inner.read().unwrap();
        let set: HashSet<&CacheKey> = hashes.iter().collect();
        let mut found = HashSet::new();
        for s in inner.search_log.values() {
            if set.contains(&s.query_hash) {
                found.insert(s.query_hash.clone());
            }
        }
        Ok(found.into_iter().collect())
    }

    async fn history_stats(&self, filter: &HistoryFilter) -> Result<HistoryStats, StoreError> {
        let inner = self.inner.read().unwrap();
        let now = Utc::now();
        let trailing_24h = now - chrono::Duration::hours(24);
        let midnight = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap();

        let searches_total = inner.search_log.len() as u64;
        let searches_24h = inner
            .search_log
            .values()
            .filter(|s| s.ts >= trailing_24h)
            .count() as u64;
        let clicks_today = inner.clicks.values().filter(|c| c.ts >= midnight).count() as u64;

        // Matching: search_log + answer_log matching filter
        let mut matching = 0;
        for s in inner.search_log.values() {
            if let Some(since) = filter.since {
                if s.ts < since {
                    continue;
                }
            }
            if let Some(q) = &filter.q {
                if !s.query.contains(q) {
                    continue;
                }
            }
            if let Some(origin) = filter.origin {
                if s.origin != origin {
                    continue;
                }
            }
            if filter.cached {
                match inner.cache.get(&s.query_hash) {
                    Some(entry) if entry.expires_at > now => {}
                    _ => continue,
                }
            }
            matching += 1;
        }

        if !filter.cached {
            for a in inner.answer_log.values() {
                if let Some(since) = filter.since {
                    if a.ts < since {
                        continue;
                    }
                }
                if let Some(q) = &filter.q {
                    if !a.query.contains(q) {
                        continue;
                    }
                }
                if let Some(origin) = filter.origin {
                    if a.origin != origin {
                        continue;
                    }
                }
                matching += 1;
            }
        }

        Ok(HistoryStats {
            searches_24h,
            searches_total,
            clicks_today,
            matching,
        })
    }

    async fn suggest(&self, prefix: &str, limit: u32) -> Result<Vec<String>, StoreError> {
        let norm_prefix = normalize_query(prefix);
        if norm_prefix.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let inner = self.inner.read().unwrap();
        // Group by query: count and latest ts
        let mut grouped: HashMap<String, (u64, DateTime<Utc>)> = HashMap::new();
        for s in inner.search_log.values() {
            let norm_query = normalize_query(&s.query);
            if norm_query.starts_with(&norm_prefix) {
                let entry = grouped.entry(norm_query).or_insert((0, s.ts));
                entry.0 += 1;
                if s.ts > entry.1 {
                    entry.1 = s.ts;
                }
            }
        }

        let mut results: Vec<(String, u64, DateTime<Utc>)> = grouped
            .into_iter()
            .map(|(q, (count, latest))| (q, count, latest))
            .collect();

        // Ranked by frecency: count desc, latest desc
        results.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
        results.truncate(limit as usize);

        Ok(results.into_iter().map(|(q, _, _)| q).collect())
    }

    async fn delete_search_log(&self, id: i64) -> Result<Option<DeleteSearchLog>, StoreError> {
        let mut inner = self.inner.write().unwrap();
        let Some(removed) = inner.search_log.remove(&id) else {
            return Ok(None);
        };

        // Cascade delete clicks only if no other search_log shares query_hash
        let shares_hash = inner
            .search_log
            .values()
            .any(|s| s.query_hash == removed.query_hash);

        let mut clicks_removed = 0;
        if !shares_hash {
            let click_keys_to_remove: Vec<i64> = inner
                .clicks
                .iter()
                .filter(|(_, c)| c.query_hash.as_ref() == Some(&removed.query_hash))
                .map(|(k, _)| *k)
                .collect();
            clicks_removed = click_keys_to_remove.len() as u64;
            for k in click_keys_to_remove {
                inner.clicks.remove(&k);
            }
        }

        Ok(Some(DeleteSearchLog {
            query: removed.query,
            clicks_removed,
        }))
    }

    // ---- stats --------------------------------------------------------------

    async fn stats(&self, days: u32) -> Result<StatsSnapshot, StoreError> {
        let inner = self.inner.read().unwrap();
        let now = Utc::now();
        let since = if days == 0 {
            DateTime::<Utc>::MIN_UTC
        } else {
            now - chrono::Duration::days(days as i64)
        };

        let window_searches: Vec<&SearchLogRow> = inner
            .search_log
            .values()
            .filter(|s| s.ts >= since)
            .collect();

        let searches = window_searches.len() as u64;
        let cache_hits = window_searches
            .iter()
            .filter(|s| s.source == LogSource::Cache)
            .count() as u64;
        let deadline_hits = window_searches.iter().filter(|s| s.deadline_hit).count() as u64;

        let hit_rate = if searches == 0 {
            0.0
        } else {
            cache_hits as f64 / searches as f64
        };

        let latency = if searches == 0 {
            None
        } else {
            let mut latencies: Vec<u32> = window_searches.iter().map(|s| s.latency_ms).collect();
            latencies.sort_unstable();
            Some(LatencyPercentiles {
                p50_ms: percentile(&latencies, 50),
                p90_ms: percentile(&latencies, 90),
                p99_ms: percentile(&latencies, 99),
            })
        };

        // Client counts
        let mut client_map: HashMap<String, u64> = HashMap::new();
        for s in &window_searches {
            *client_map.entry(s.client.label().to_string()).or_default() += 1;
        }
        let mut by_client: Vec<ClientCount> = client_map
            .into_iter()
            .map(|(client, count)| ClientCount {
                client,
                searches: count,
            })
            .collect();
        by_client.sort_by(|a, b| b.searches.cmp(&a.searches));

        // Top queries
        let mut query_map: HashMap<String, u64> = HashMap::new();
        for s in &window_searches {
            *query_map.entry(s.query.clone()).or_default() += 1;
        }
        let mut top_queries: Vec<QueryCount> = query_map
            .into_iter()
            .map(|(query, count)| QueryCount {
                query,
                searches: count,
            })
            .collect();
        top_queries.sort_by(|a, b| b.searches.cmp(&a.searches));
        top_queries.truncate(10);

        // Zero-result queries
        let mut zero_map: HashMap<String, u64> = HashMap::new();
        for s in window_searches.iter().filter(|s| s.result_count == 0) {
            *zero_map.entry(s.query.clone()).or_default() += 1;
        }
        let mut zero_vec: Vec<(String, u64)> = zero_map.into_iter().collect();
        zero_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let zero_result_queries = zero_vec.into_iter().map(|(q, _)| q).collect();

        // Hits by tier
        let mut tier_map: HashMap<u8, u64> = HashMap::new();
        for s in window_searches
            .iter()
            .filter(|s| s.source == LogSource::Cache)
        {
            if let Some(tier) = s.tier {
                *tier_map.entry(tier.as_u8()).or_default() += 1;
            }
        }
        let mut hits_by_tier: Vec<TierHit> = tier_map
            .into_iter()
            .map(|(tier, hits)| TierHit { tier, hits })
            .collect();
        hits_by_tier.sort_by_key(|t| t.tier);

        // Per day
        let mut day_map: BTreeMap<chrono::NaiveDate, (u64, u64)> = BTreeMap::new();
        for s in &window_searches {
            let day = s.ts.date_naive();
            let entry = day_map.entry(day).or_default();
            entry.0 += 1;
            if s.source == LogSource::Cache {
                entry.1 += 1;
            }
        }
        let per_day: Vec<DayCount> = day_map
            .into_iter()
            .map(|(day, (s, h))| DayCount {
                day,
                searches: s,
                cache_hits: h,
            })
            .collect();

        // Engine health rows
        let mut engines: Vec<EngineStatsRow> = inner
            .health
            .values()
            .cloned()
            .map(EngineStatsRow::from_health)
            .collect();
        engines.sort_by(|a, b| a.engine.cmp(&b.engine));

        // Cache panel
        let cache_entries = inner
            .cache
            .values()
            .filter(|c| c.expires_at > now)
            .count() as u64;
        let cache_entries_expired = inner
            .cache
            .values()
            .filter(|c| c.expires_at <= now)
            .count() as u64;
        let cache_newest_at = inner.cache.values().map(|c| c.created_at).max();

        Ok(StatsSnapshot {
            window_days: days,
            searches,
            cache_hits,
            hit_rate,
            latency,
            ttfr: None,
            by_client,
            top_queries,
            zero_result_queries,
            hits_by_tier,
            outcomes: BTreeMap::new(),
            deadline_hits,
            per_day,
            engines,
            cache_entries,
            cache_entries_expired,
            cache_db_bytes: 0,
            cache_newest_at,
            admission: AdmissionStats::default(),
            engine_eval: None,
        })
    }

    // ---- engine health ------------------------------------------------------

    async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut list: Vec<EngineHealthRow> = inner.health.values().cloned().collect();
        list.sort_by(|a, b| a.engine.cmp(&b.engine));
        Ok(list)
    }

    async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.health.insert(row.engine.clone(), row.clone());
        Ok(())
    }

    // ---- audit --------------------------------------------------------------

    async fn audit(&self, mut row: AuditRow) -> Result<(), StoreError> {
        let mut inner = self.inner.write().unwrap();
        inner.audit_seq += 1;
        let id = inner.audit_seq;
        row.id = Some(id);
        inner.audit.insert(id, row);
        Ok(())
    }

    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut rows = Vec::new();
        for r in inner.audit.values() {
            if let Some(since) = filter.since {
                if r.ts < since {
                    continue;
                }
            }
            if let Some(actor) = &filter.actor {
                if &r.actor != actor {
                    continue;
                }
            }
            if let Some(action) = &filter.action {
                if &r.action != action {
                    continue;
                }
            }
            rows.push(r.clone());
        }
        // Newest first (ts desc, id desc)
        rows.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| b.id.cmp(&a.id)));
        rows.truncate(filter.limit as usize);
        Ok(rows)
    }

    async fn audit_facets(&self) -> Result<AuditFacets, StoreError> {
        let inner = self.inner.read().unwrap();
        let mut actors = HashSet::new();
        let mut actions = HashSet::new();
        for r in inner.audit.values() {
            actors.insert(r.actor.clone());
            actions.insert(r.action.clone());
        }
        let mut actors_vec: Vec<String> = actors.into_iter().collect();
        let mut actions_vec: Vec<String> = actions.into_iter().collect();
        actors_vec.sort();
        actions_vec.sort();
        Ok(AuditFacets {
            actors: actors_vec,
            actions: actions_vec,
        })
    }
}
