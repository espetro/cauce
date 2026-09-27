//! Response-side types: `SearchResult`, `SearchMeta`, `SearchResponse`,
//! `Source`, `EngineReport`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use url::Url;
use uuid::Uuid;

use crate::engine::{EngineError, EngineId, Tier};

/// Where a response came from (parent plan 4.2, `meta.source`).
///
/// Wire shape is externally tagged with snake_case variant names:
/// `{"cache":{"tier":1,"age_s":12,"ttl_s":3600,"stale":false}}` or `"network"`.
/// A fuzzy hit additionally carries `matched_query` (W1-10 tier-2 lexical,
/// W5-05 tier-3 semantic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Cache {
        /// Serializes as the tier integer (1-3).
        #[ts(type = "number")]
        tier: Tier,
        /// Seconds since the hit was stored.
        #[ts(type = "number")]
        age_s: u64,
        /// Seconds until expiry (remaining TTL at serve time).
        #[ts(type = "number")]
        ttl_s: u64,
        /// True when the row was past `expires_at` (served stale).
        stale: bool,
        /// The stored query a fuzzy tier (2 lexical, 3 semantic) matched.
        /// Absent on an exact tier-1 hit, so the tier-1 wire shape is
        /// unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        matched_query: Option<String>,
    },
    Network,
}

/// Per-engine outcome folded into `SearchMeta::engines_used`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    Ok,
    Failed(EngineError),
}

/// Per-engine report of a fan-out: status, observed latency, result count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EngineReport {
    pub engine: EngineId,
    pub status: EngineStatus,
    pub latency_ms: u32,
    pub result_count: u32,
}

/// One merged result row (parent plan 4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SearchResult {
    pub url: Url,
    pub title: String,
    pub snippet: String,
    /// Engine that produced the row (before merge dedupe keeps the best score).
    pub engine: EngineId,
    /// Publication timestamp when the engine reports one; RFC 3339 on the wire.
    pub published: Option<DateTime<Utc>>,
    /// RRF score assigned by the merge; raw engine order otherwise.
    pub score: f32,
}

/// Response metadata (parent plan 4.2). `request_id` is a UUIDv7 minted by the
/// inbound surface and echoed in `X-Request-Id` and the JSONL log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SearchMeta {
    pub source: Source,
    pub engines_used: Vec<EngineReport>,
    /// Engine ids suppressed by an open breaker on this request (W2-01
    /// amendment): collected at the fan-out gate and surfaced on the page
    /// meta line and the SSE `meta` event. `default` keeps `payload_json`
    /// rows written before the field existed decodable.
    #[serde(default)]
    pub engines_skipped: Vec<EngineId>,
    /// True when the hard deadline cancelled at least one engine call.
    pub deadline_hit: bool,
    /// True when the W3-01 hedge fired tier-2 engines on this request.
    /// `default` keeps `payload_json` rows written before the field
    /// existed decodable.
    #[serde(default)]
    pub hedged: bool,
    /// Elapsed ms at which the hedge fired; `None` when it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hedge_at_ms: Option<u32>,
    pub elapsed_ms: u32,
    pub request_id: Uuid,
}

/// The canonical `GET /api/search` payload (parent plan 4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SearchResponse {
    /// The query as executed (post-normalisation).
    pub query: String,
    pub results: Vec<SearchResult>,
    pub meta: SearchMeta,
}

/// One event of [`crate::SearchPipeline::search_stream`] (W2-01):
/// `Results` batches arrive per engine in return order; `Meta` is the
/// terminal event on success, `Error` on failure. The channel closes after
/// the terminal event.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// One engine's batch: `results` is that engine's raw page (pre-merge);
    /// `elapsed_ms` is measured from request start. Whole-response serves
    /// (tier-1/2 hit, stale overflow, singleflight join) emit one batch per
    /// producing engine instead.
    Results {
        engine: EngineId,
        results: Vec<SearchResult>,
        elapsed_ms: u32,
    },
    /// Terminal metadata — the shared [`SearchMeta`] plus `order`.
    Meta(StreamMeta),
    /// Terminal failure; the inbound surface maps the typed error to its
    /// wire code (HTTP status for `GET /api/search`, the SSE `error`
    /// payload for `GET /api/search/stream`).
    Error(crate::pipeline::PipelineError),
}

/// The `meta` event payload of the search stream (W2-01): the canonical
/// [`SearchMeta`] flattened, plus `order` — the final RRF ordering of the
/// merged results as dedupe keys (each row's `normalize_url` form, the
/// same key the per-result `key` field and the merge itself use), best
/// first. The progressive page never reorders what it rendered; `order`
/// is what lets it tell the user how many late results would have ranked
/// above the visible ones, and hide anything the merge did not keep.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct StreamMeta {
    #[serde(flatten)]
    #[ts(flatten)]
    pub meta: SearchMeta,
    /// Final RRF order: the emitted results' dedupe keys, best first.
    pub order: Vec<Url>,
}

/// A [`SearchResult`] plus `key`, the client-side dedupe key
/// (`normalize_url` of its URL — the same form `meta.order` carries). The
/// progressive page dedupes appended articles on `key`: the merge drops
/// duplicate URL spellings the raw `url` field would render twice.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct StreamResult {
    /// Dedup key: `normalize_url` of `url`.
    pub key: Url,
    #[serde(flatten)]
    #[ts(flatten)]
    pub result: SearchResult,
}

impl From<&SearchResult> for StreamResult {
    fn from(result: &SearchResult) -> Self {
        Self {
            key: crate::normalize_url(&result.url),
            result: result.clone(),
        }
    }
}

/// The `results` SSE event payload of `GET /api/search/stream` (W2-01):
/// one engine's raw batch (pre-merge) plus the request-elapsed ms.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ResultsFrame {
    pub engine: EngineId,
    pub results: Vec<StreamResult>,
    pub elapsed_ms: u32,
}
