//! Response-side types: `SearchResult`, `SearchMeta`, `SearchResponse`,
//! `Source`, `EngineReport`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::engine::{EngineError, EngineId, Tier};

/// Where a response came from (parent plan 4.2, `meta.source`).
///
/// Wire shape is externally tagged with snake_case variant names:
/// `{"cache":{"tier":1,"age_s":12,"ttl_s":3600,"stale":false}}` or `"network"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Cache {
        tier: Tier,
        /// Seconds since the hit was stored.
        age_s: u64,
        /// Seconds until expiry (remaining TTL at serve time).
        ttl_s: u64,
        /// True when the row was past `expires_at` (served stale).
        stale: bool,
    },
    Network,
}

/// Per-engine outcome folded into `SearchMeta::engines_used`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    Ok,
    Failed(EngineError),
}

/// Per-engine report of a fan-out: status, observed latency, result count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineReport {
    pub engine: EngineId,
    pub status: EngineStatus,
    pub latency_ms: u32,
    pub result_count: u32,
}

/// One merged result row (parent plan 4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchMeta {
    pub source: Source,
    pub engines_used: Vec<EngineReport>,
    /// True when the hard deadline cancelled at least one engine call.
    pub deadline_hit: bool,
    pub elapsed_ms: u32,
    pub request_id: Uuid,
}

/// The canonical `GET /api/search` payload (parent plan 4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResponse {
    /// The query as executed (post-normalisation).
    pub query: String,
    pub results: Vec<SearchResult>,
    pub meta: SearchMeta,
}
