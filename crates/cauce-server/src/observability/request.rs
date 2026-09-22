//! `RequestId` (UUIDv7) and the span glue that carries it.
//!
//! Every inbound request gets one `RequestId`: HTTP middleware (W0-09),
//! MCP (W1-08) and CLI (W0-12) mint it, enter a [`request_span`] for the
//! request duration and echo it in `meta.request_id`, `X-Request-Id` and
//! MCP tool results (parent plan 6.1). The JSONL layer hoists the
//! `request_id` span field to a top-level field on every line.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-request identifier: a UUIDv7 (time-ordered, monotonic enough for
/// JSONL timelines).
///
/// `Copy` and `Display`/`FromStr` via the canonical hyphenated form so it
/// can be passed straight into `SearchMeta::request_id` and compared
/// against `cauce trace <id>` arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(Uuid);

impl RequestId {
    /// Mint a fresh UUIDv7.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Wrap an existing UUID (e.g. an inbound `X-Request-Id` a caller chose
    /// to trust).
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The wrapped UUID (`SearchMeta::request_id` is a plain `Uuid`).
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<RequestId> for Uuid {
    fn from(id: RequestId) -> Uuid {
        id.0
    }
}

impl From<Uuid> for RequestId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RequestId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s.trim()).map(Self)
    }
}

/// Root span for one inbound request. Carries `request_id` as a span field
/// so every event in the request tree is attributable; `client`, `method`
/// and `path` are declared empty for the inbound surface to `record` once
/// known. `otel.kind = "server"` marks it as a server span for OTLP export.
///
/// ```
/// # use cauce_server::observability::{request_span, RequestId};
/// let span = request_span(RequestId::new());
/// let _entered = span.enter();
/// tracing::info!("inside the request");
/// ```
pub fn request_span(request_id: RequestId) -> tracing::Span {
    tracing::info_span!(
        "request",
        request_id = %request_id,
        otel.kind = "server",
        client = tracing::field::Empty,
        method = tracing::field::Empty,
        path = tracing::field::Empty,
    )
}
