//! Process metrics handle (W1-09): the owned in-process registry lives in
//! `oxe_core::metrics`; this handle only bridges the async `Store` into the
//! `oxe_cache_entries` gauge and hands out the `Metrics` record handle.
//!
//! `oxe_cache_entries` is a refreshed cell rather than a direct `Store`
//! read: the `/metrics` render is synchronous and cannot await the store,
//! so the handler refreshes the cell before each scrape and
//! [`MetricsHandle::render`] just formats what is there.
//!
//! OTLP export of traces stays opt-in behind the non-default `otlp` cargo
//! feature (`observability::otlp`); metrics have no push path in this step.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use oxe_core::Metrics;
use oxe_core::Store;
use oxe_core::metrics::{render_prometheus, set_cache_entries};

/// Prometheus exposition content type for `GET /metrics`.
pub const METRICS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Owns the store reference backing the `oxe_cache_entries` gauge. Cheap
/// to clone (one `Arc`); the record path needs no handle at all because
/// `oxe_core::metrics` is process-global.
#[derive(Clone)]
pub struct MetricsHandle {
    store: Arc<dyn Store>,
}

impl MetricsHandle {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// The [`Metrics`] record handle (for `SearchPipeline::with_metrics`
    /// wiring; recording works without it since the registry is global).
    pub fn metrics(&self) -> Metrics {
        Metrics
    }

    /// Refresh the `oxe_cache_entries` cell from the store. Called by the
    /// `/metrics` handler before each scrape.
    pub async fn refresh_cache(&self) {
        // days=0 reads the unbounded window; only the cache counts are used.
        if let Ok(stats) = self.store.stats(0).await {
            set_cache_entries(stats.cache_entries);
        }
    }

    /// Prometheus text exposition of everything recorded so far.
    /// Infallible by construction: an empty registry still produces a
    /// parseable document.
    pub fn render(&self) -> String {
        render_prometheus()
    }
}
