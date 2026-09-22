//! Periodic `evict_expired` on a tokio interval (parent plan section 5:
//! every 5 minutes). The server wires `spawn_eviction_task` in W0-09.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use oxe_core::Store;
use tokio::task::JoinHandle;

use crate::SqliteStore;

/// How often the background task deletes expired `cache_entries` rows.
pub const EVICTION_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Spawn the background eviction loop: `evict_expired` every 5 minutes.
/// Cancel the returned handle to stop it; the first tick runs immediately,
/// which is harmless (it deletes nothing on a fresh database).
pub fn spawn_eviction_task(store: Arc<SqliteStore>) -> JoinHandle<()> {
    spawn_eviction_task_every(store, EVICTION_INTERVAL)
}

/// Same loop with a caller-chosen period. Public (but hidden) so tests can
/// run it on a millisecond cadence.
#[doc(hidden)]
pub fn spawn_eviction_task_every(store: Arc<SqliteStore>, period: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(period);
        loop {
            tick.tick().await;
            match store.evict_expired().await {
                Ok(0) => {}
                Ok(n) => tracing::info!(evicted = n, "evicted expired cache entries"),
                Err(e) => tracing::warn!(error = %e, "cache eviction failed"),
            }
        }
    })
}
