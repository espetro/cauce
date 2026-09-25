//! Recording in-memory `Store` stub for the `SearchPipeline`
//! integration tests.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cauce_core::{
    AnswerKey, AnswerRow, AuditFilter, AuditRow, CacheKey, CacheState, CachedAnswer, CachedSearch,
    ClickRow, DeleteSearchLog, EngineHealthRow, EngineStatus, HistoryFilter, HistoryItem,
    HistoryStats, PageRow, SearchLogRow, SearchResponse, StatsSnapshot, Store, StoreError,
};
use chrono::{DateTime, Utc};

/// What `put` stored: response, write time and TTL.
type StoredEntry = (SearchResponse, DateTime<Utc>, Duration);

/// What `put_answer` stored: row, write time and TTL.
type StoredAnswer = (AnswerRow, DateTime<Utc>, Duration);

fn to_cached_answer(key: &str, (row, created, ttl): &StoredAnswer) -> CachedAnswer {
    CachedAnswer {
        key: key.parse().expect("stored keys are AnswerKey hex"),
        query: row.query.clone(),
        model: row.model.clone(),
        payload: row.payload.clone(),
        sources: row.sources.clone(),
        created_at: *created,
        expires_at: *created + chrono::Duration::from_std(*ttl).unwrap(),
    }
}

/// Rebuild the `CachedSearch` a real store would decode for a `put` row
/// (expired rows included; `get_exact` filters them itself).
fn to_cached(key: &str, (resp, created, ttl): &StoredEntry) -> CachedSearch {
    CachedSearch {
        key: key.parse().expect("stored keys are CacheKey hex"),
        query: resp.query.clone(),
        params: serde_json::json!({ "q": resp.query }),
        response: resp.clone(),
        created_at: *created,
        expires_at: *created + chrono::Duration::from_std(*ttl).unwrap(),
        hits: 1,
        engines: resp
            .meta
            .engines_used
            .iter()
            .filter(|r| matches!(r.status, EngineStatus::Ok))
            .map(|r| r.engine.clone())
            .collect(),
    }
}

/// Recording in-memory `Store`: serves `get_exact` from what `put` wrote,
/// captures every `put`/`log_search` call. Unused methods
/// `unimplemented!()` so a stray call panics the test.
#[derive(Default)]
pub struct StubStore {
    pub entries: Mutex<HashMap<String, StoredEntry>>,
    pub puts: Mutex<Vec<(String, Duration)>>,
    pub logs: Mutex<Vec<SearchLogRow>>,
    /// Latest `engine_health` row per engine id (as `put_health` wrote it).
    pub health_rows: Mutex<HashMap<String, EngineHealthRow>>,
    /// Every `put_health` call, in order (debounce assertions).
    pub health_writes: Mutex<Vec<EngineHealthRow>>,
    pub audits: Mutex<Vec<AuditRow>>,
    /// `answers` table stand-in (W4-02): rows by `AnswerKey` hex.
    pub answers: Mutex<HashMap<String, StoredAnswer>>,
    /// `pages` table stand-in (W5-01): rows by normalized URL.
    pub pages: Mutex<HashMap<String, PageRow>>,
    pub fail_get: AtomicBool,
    pub fail_lexical: AtomicBool,
    /// Extra delay inside `get_lexical` — simulates slow pre-fan-out
    /// work for tests that care what instant the hedge clock starts at.
    pub lexical_delay_ms: AtomicU64,
}

#[async_trait]
impl Store for StubStore {
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err(StoreError::Backend("injected lookup failure".to_string()));
        }
        let entries = self.entries.lock().unwrap();
        Ok(entries.get(key.as_str()).and_then(|entry| {
            (to_cached(key.as_str(), entry).expires_at > Utc::now())
                .then(|| to_cached(key.as_str(), entry))
        }))
    }

    /// FTS stand-in: rows whose stored query shares a whitespace token with
    /// `q`, expired included (the pinned `get_lexical` semantic). The
    /// pipeline's own gate decides acceptance, so a permissive candidate
    /// list is what the tier-2 tests want.
    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError> {
        if self.fail_lexical.load(Ordering::SeqCst) {
            return Err(StoreError::Backend("injected lexical failure".to_string()));
        }
        let delay = self.lexical_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let want: HashSet<String> = q.split_whitespace().map(|t| t.to_lowercase()).collect();
        let mut rows: Vec<CachedSearch> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, (resp, _, _))| {
                resp.query
                    .split_whitespace()
                    .any(|t| want.contains(&t.to_lowercase()))
            })
            .map(|(key, entry)| to_cached(key, entry))
            .collect();
        // Deterministic order so tests do not depend on HashMap iteration.
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        rows.truncate(usize::from(limit));
        Ok(rows)
    }

    async fn put(
        &self,
        key: &CacheKey,
        resp: &SearchResponse,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        self.puts
            .lock()
            .unwrap()
            .push((key.as_str().to_string(), ttl));
        self.entries
            .lock()
            .unwrap()
            .insert(key.as_str().to_string(), (resp.clone(), Utc::now(), ttl));
        Ok(())
    }

    /// Honour the W3-02 grace window: an expired row only leaves once it
    /// is `grace` past expiry (the pipeline still serves it stale until
    /// then).
    async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError> {
        let cutoff =
            Utc::now() - chrono::Duration::from_std(grace).unwrap_or(chrono::Duration::MAX);
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|key, entry| to_cached(key, entry).expires_at > cutoff);
        Ok((before - entries.len()) as u64)
    }
    async fn list_cache(&self, _: u32, _: u32) -> Result<Vec<CachedSearch>, StoreError> {
        unimplemented!()
    }
    /// `get_exact` hides expired rows; `get_cache` returns them (stale
    /// serving is the admission layer's call).
    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        let entries = self.entries.lock().unwrap();
        Ok(entries
            .get(key.as_str())
            .map(|entry| to_cached(key.as_str(), entry)))
    }
    async fn delete_cache(&self, _: &CacheKey) -> Result<bool, StoreError> {
        unimplemented!()
    }
    async fn clear_cache(&self) -> Result<u64, StoreError> {
        unimplemented!()
    }

    /// `get_exact`-style TTL honouring for `answers`.
    async fn get_answer(&self, key: &AnswerKey) -> Result<Option<CachedAnswer>, StoreError> {
        let answers = self.answers.lock().unwrap();
        Ok(answers.get(key.as_str()).and_then(|row| {
            (to_cached_answer(key.as_str(), row).expires_at > Utc::now())
                .then(|| to_cached_answer(key.as_str(), row))
        }))
    }

    async fn put_answer(
        &self,
        key: &AnswerKey,
        row: &AnswerRow,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        self.answers
            .lock()
            .unwrap()
            .insert(key.as_str().to_string(), (row.clone(), Utc::now(), ttl));
        Ok(())
    }

    async fn log_search(&self, row: SearchLogRow) -> Result<(), StoreError> {
        self.logs.lock().unwrap().push(row);
        Ok(())
    }

    async fn record_click(&self, _: ClickRow) -> Result<(), StoreError> {
        unimplemented!()
    }
    async fn list_history(&self, _: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError> {
        unimplemented!()
    }
    async fn cache_states(&self, _: &[CacheKey]) -> Result<Vec<CacheState>, StoreError> {
        unimplemented!()
    }
    async fn search_hashes(&self, _: &[CacheKey]) -> Result<Vec<CacheKey>, StoreError> {
        unimplemented!()
    }
    async fn history_stats(&self, _: &HistoryFilter) -> Result<HistoryStats, StoreError> {
        unimplemented!()
    }
    async fn suggest(&self, _: &str, _: u32) -> Result<Vec<String>, StoreError> {
        unimplemented!()
    }
    async fn delete_search_log(&self, _: i64) -> Result<Option<DeleteSearchLog>, StoreError> {
        unimplemented!()
    }
    async fn put_page(&self, row: &PageRow) -> Result<(), StoreError> {
        self.pages
            .lock()
            .unwrap()
            .insert(row.url.as_str().to_string(), row.clone());
        Ok(())
    }

    async fn get_page(&self, url: &url::Url) -> Result<Option<PageRow>, StoreError> {
        Ok(self.pages.lock().unwrap().get(url.as_str()).cloned())
    }

    async fn stats(&self, _: u32) -> Result<StatsSnapshot, StoreError> {
        unimplemented!()
    }
    async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
        Ok(self.health_rows.lock().unwrap().values().cloned().collect())
    }
    async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError> {
        self.health_writes.lock().unwrap().push(row.clone());
        self.health_rows
            .lock()
            .unwrap()
            .insert(row.engine.as_str().to_string(), row.clone());
        Ok(())
    }
    async fn audit(&self, row: AuditRow) -> Result<(), StoreError> {
        self.audits.lock().unwrap().push(row);
        Ok(())
    }
    async fn list_audit(&self, _: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
        unimplemented!()
    }
}
