//! Shared test support for the `SearchPipeline` integration tests: a
//! request builder, a `Replay` constructor, and a recording in-memory
//! `Store` stub.
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
    AuditFilter, AuditRow, CacheKey, CachedSearch, ClickRow, ClientKind, Engine, EngineError,
    EngineHealthRow, EngineId, EngineStatus, HistoryFilter, HistoryItem, SafeSearch, SearchLogRow,
    SearchRequest, SearchResponse, StatsSnapshot, Store, StoreError, Tier,
};
use cauce_engines::{Replay, ReplayOpts};
use chrono::{DateTime, Utc};

pub fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Api,
    }
}

/// A `Replay` engine rooted at `root` (no cassettes found → synthetic
/// mode, or cassette dirs created by the test).
pub fn replay_at(root: &std::path::Path, f: impl FnOnce(&mut ReplayOpts)) -> Replay {
    let mut opts = ReplayOpts {
        fixtures_root: root.to_path_buf(),
        ..ReplayOpts::default()
    };
    f(&mut opts);
    Replay::new(opts)
}

/// A `Replay` behind a health gate: while `healthy` is false every call
/// fails with `EngineError::Blocked`; flipping it lets the next call
/// through, which is what breaker-probe tests need (unlike the immutable
/// `ReplayOpts::blocked`). Used only by `health.rs`; each test binary
/// compiles this module separately.
#[allow(dead_code)]
pub struct GateEngine {
    inner: Replay,
    healthy: AtomicBool,
    calls: AtomicU64,
}

#[allow(dead_code)]
impl GateEngine {
    /// `healthy = false` → every call returns `Blocked`.
    pub fn new(inner: Replay, healthy: bool) -> Self {
        Self {
            inner,
            healthy: AtomicBool::new(healthy),
            calls: AtomicU64::new(0),
        }
    }

    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }

    /// `search` calls seen, blocked ones included.
    pub fn call_count(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Engine for GateEngine {
    fn id(&self) -> EngineId {
        self.inner.id()
    }
    fn tier(&self) -> Tier {
        self.inner.tier()
    }
    fn page_size(&self) -> u8 {
        self.inner.page_size()
    }
    async fn search(
        &self,
        req: &SearchRequest,
        budget: Duration,
    ) -> Result<Vec<cauce_core::SearchResult>, EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.healthy.load(Ordering::SeqCst) {
            return Err(EngineError::Blocked);
        }
        self.inner.search(req, budget).await
    }
}

/// What `put` stored: response, write time and TTL.
type StoredEntry = (SearchResponse, DateTime<Utc>, Duration);

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
    pub fail_get: AtomicBool,
    pub fail_lexical: AtomicBool,
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

    async fn evict_expired(&self) -> Result<u64, StoreError> {
        unimplemented!()
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
