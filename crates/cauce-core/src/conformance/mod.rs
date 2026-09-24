//! Reusable conformance suite for `Store` implementations.
//!
//! Enabled by the `conformance` cargo feature (default off; `cauce-core` stays
//! dependency-free of test-only code in normal builds). Each `pub async fn`
//! exercises one area of the `Store` contract and panics with context on any
//! violation. `cauce-store-sqlite` (W0-04) calls them against a temp file; the
//! Postgres impl (W6) must pass the same suite.
//!
//! Each function is written to tolerate pre-existing rows (it asserts deltas
//! or membership rather than whole-table equality), so `run_all` can exercise
//! them all against a single store instance.
//!
//! Module map: `mod.rs` holds the shared fixture builders ([`request`],
//! [`response`], [`log_row`]) plus [`run_all`], which invokes every cluster in
//! order. The per-area suites live beside it: [`cache`] — exact round-trip,
//! expiry/eviction and the admin surface; [`lexical`] — the tier-2 FTS lookup;
//! [`history`] — search log, clicks and the merged feed; [`stats`] —
//! dashboard aggregates; [`health`] — engine health upserts; [`audit`] — the
//! audit trail and its facets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod audit;
mod cache;
mod health;
mod history;
mod lexical;
mod stats;

pub use audit::audit_trail;
pub use cache::{cache_admin, cache_exact_roundtrip, cache_expiry_and_eviction};
pub use health::engine_health;
pub use history::log_clicks_history;
pub use lexical::lexical_search;
pub use stats::stats_aggregates;

use chrono::{DateTime, Utc};
use url::Url;
use uuid::Uuid;

use crate::Tier;
use crate::cache::CacheKey;
use crate::engine::EngineId;
use crate::request::{ClientKind, SafeSearch, SearchRequest};
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source,
};
use crate::store::{LogSource, SearchLogRow, Store};

/// A deterministic request for `CacheKey` generation. The `q` value should be
/// unique per test area so shared-store runs cannot cross-contaminate.
pub fn request(q: &str) -> SearchRequest {
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

/// A `SearchResponse` with `(title, url, snippet)` result tuples produced by a
/// single `conf-engine` report. Deterministic except for `request_id`.
pub fn response(q: &str, results: &[(&str, &str, &str)]) -> SearchResponse {
    let results: Vec<SearchResult> = results
        .iter()
        .enumerate()
        .map(|(i, (title, url, snippet))| SearchResult {
            url: Url::parse(url).expect("fixture url must parse"),
            title: title.to_string(),
            snippet: snippet.to_string(),
            engine: EngineId::from("conf-engine"),
            published: None,
            score: 1.0 - i as f32 * 0.1,
        })
        .collect();
    SearchResponse {
        query: q.to_string(),
        meta: SearchMeta {
            source: Source::Network,
            engines_used: vec![EngineReport {
                engine: EngineId::from("conf-engine"),
                status: EngineStatus::Ok,
                latency_ms: 12,
                result_count: results.len() as u32,
            }],
            engines_skipped: Vec::new(),
            deadline_hit: false,
            hedged: false,
            hedge_at_ms: None,
            elapsed_ms: 12,
            request_id: Uuid::now_v7(),
        },
        results,
    }
}

/// A `SearchLogRow` builder with the fields the suite varies; `id` is `None`
/// as on a real insert.
pub fn log_row(
    ts: DateTime<Utc>,
    query: &str,
    client: ClientKind,
    source: LogSource,
    latency_ms: u32,
    result_count: u32,
) -> SearchLogRow {
    SearchLogRow {
        id: None,
        ts,
        query_hash: CacheKey::from(&request(query)),
        query: query.to_string(),
        query_raw: Some(query.to_string()),
        client,
        source,
        tier: match source {
            LogSource::Cache => Some(Tier::T1),
            LogSource::Network => None,
        },
        latency_ms,
        result_count,
        engines: vec![EngineId::from("conf-engine")],
        deadline_hit: false,
    }
}

/// Run the whole suite against one store. Safe on a non-empty store: every
/// function uses unique fixture values and asserts membership or deltas.
/// `cache_admin` runs last because it clears `cache_entries`.
pub async fn run_all(store: &impl Store) {
    cache_exact_roundtrip(store).await;
    cache_expiry_and_eviction(store).await;
    lexical_search(store).await;
    log_clicks_history(store).await;
    stats_aggregates(store).await;
    engine_health(store).await;
    audit_trail(store).await;
    cache_admin(store).await;
}
