//! Shared test support for the `SearchPipeline` integration tests: a
//! request builder, a `Replay` constructor, and a recording in-memory
//! `Store` stub.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use oxe_core::{
    AuditFilter, AuditRow, CacheKey, CachedSearch, ClickRow, ClientKind, Engine, EngineError,
    EngineHealthRow, EngineId, EngineStatus, HistoryFilter, HistoryItem, SafeSearch, SearchLogRow,
    SearchRequest, SearchResponse, StatsSnapshot, Store, StoreError, Tier,
};
use oxe_engines::{Replay, ReplayOpts};

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
    ) -> Result<Vec<oxe_core::SearchResult>, EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.healthy.load(Ordering::SeqCst) {
            return Err(EngineError::Blocked);
        }
        self.inner.search(req, budget).await
    }
}

/// What `put` stored: response, write time and TTL.
type StoredEntry = (SearchResponse, DateTime<Utc>, Duration);

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
}

#[async_trait]
impl Store for StubStore {
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err(StoreError::Backend("injected lookup failure".to_string()));
        }
        let entries = self.entries.lock().unwrap();
        Ok(entries.get(key.as_str()).and_then(|(resp, created, ttl)| {
            let expires = *created + chrono::Duration::from_std(*ttl).unwrap();
            (expires > Utc::now()).then(|| CachedSearch {
                key: key.clone(),
                query: resp.query.clone(),
                params: serde_json::json!({ "q": resp.query }),
                response: resp.clone(),
                created_at: *created,
                expires_at: expires,
                hits: 1,
                engines: resp
                    .meta
                    .engines_used
                    .iter()
                    .filter(|r| matches!(r.status, EngineStatus::Ok))
                    .map(|r| r.engine.clone())
                    .collect(),
            })
        }))
    }

    async fn get_lexical(&self, _: &str, _: u8) -> Result<Vec<CachedSearch>, StoreError> {
        unimplemented!()
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
    async fn get_cache(&self, _: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        unimplemented!()
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
