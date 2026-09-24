//! Engine test doubles: `GateEngine` (health gate) and `DialEngine`
//! (re-dialable latency), both wrapping a `Replay`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cauce_core::{Engine, EngineError, EngineId, SearchRequest, Tier};
use cauce_engines::Replay;

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

/// A `Replay` whose latency is re-dialable between calls: health history
/// builds at one latency while a later call measures at another — the
/// knob hedge tests need when "slow history" and "current behaviour"
/// must differ on the same engine id.
#[allow(dead_code)]
pub struct DialEngine {
    inner: Replay,
    latency_ms: AtomicU64,
    calls: AtomicU64,
}

#[allow(dead_code)]
impl DialEngine {
    pub fn new(inner: Replay, latency_ms: u64) -> Self {
        Self {
            inner,
            latency_ms: AtomicU64::new(latency_ms),
            calls: AtomicU64::new(0),
        }
    }

    pub fn set_latency_ms(&self, latency_ms: u64) {
        self.latency_ms.store(latency_ms, Ordering::SeqCst);
    }

    pub fn call_count(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Engine for DialEngine {
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
        let latency = self.latency_ms.load(Ordering::SeqCst);
        if latency > 0 {
            tokio::time::sleep(Duration::from_millis(latency).min(budget)).await;
        }
        self.inner.search(req, budget).await
    }
}
