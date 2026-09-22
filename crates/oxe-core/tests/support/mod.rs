//! Shared test support for the `SearchPipeline` integration tests: a
//! request builder, a `Replay` constructor, and a recording in-memory
//! `Store` stub.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use oxe_core::{
    AuditFilter, AuditRow, CacheKey, CachedSearch, ClickRow, ClientKind, EngineHealthRow,
    EngineStatus, HistoryFilter, HistoryItem, SafeSearch, SearchLogRow, SearchRequest,
    SearchResponse, StatsSnapshot, Store, StoreError,
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
    pub fail_get: AtomicBool,
}

impl StubStore {
    /// Row materialisation shared by `get_exact`/`get_cache`: `get_exact`
    /// hides expired rows, `get_cache` returns them (stale serving is the
    /// admission layer's call).
    fn row(&self, key: &CacheKey) -> Option<CachedSearch> {
        self.entries
            .lock()
            .unwrap()
            .get(key.as_str())
            .map(|(resp, created, ttl)| CachedSearch {
                key: key.clone(),
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
            })
    }
}

#[async_trait]
impl Store for StubStore {
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        if self.fail_get.load(Ordering::SeqCst) {
            return Err(StoreError::Backend("injected lookup failure".to_string()));
        }
        Ok(self.row(key).filter(|row| row.expires_at > Utc::now()))
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
    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
        Ok(self.row(key))
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
        unimplemented!()
    }
    async fn put_health(&self, _: &EngineHealthRow) -> Result<(), StoreError> {
        unimplemented!()
    }
    async fn audit(&self, _: AuditRow) -> Result<(), StoreError> {
        unimplemented!()
    }
    async fn list_audit(&self, _: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
        unimplemented!()
    }
}
