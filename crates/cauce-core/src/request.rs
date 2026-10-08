//! Inbound request types (`SearchRequest` and its field enums).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::EngineId;

/// Freshness window of a search (SearXNG-compatible names on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

/// Safe-search level. Serialized as `off` | `moderate` | `strict`.
///
/// `FromStr` additionally accepts the SearXNG numeric form `0` | `1` | `2`,
/// which is what `safesearch=` query parameters historically carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeSearch {
    Off,
    #[default]
    Moderate,
    Strict,
}

impl std::fmt::Display for SafeSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Moderate => "moderate",
            Self::Strict => "strict",
        })
    }
}

impl std::str::FromStr for SafeSearch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" | "0" => Ok(Self::Off),
            "moderate" | "1" => Ok(Self::Moderate),
            "strict" | "2" => Ok(Self::Strict),
            other => Err(format!("unknown safesearch level: {other}")),
        }
    }
}

impl std::str::FromStr for TimeRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            "year" => Ok(Self::Year),
            other => Err(format!("unknown time_range: {other}")),
        }
    }
}

/// Which surface a request came in through.
///
/// `Mcp` carries the MCP client name (`mcp:<name>` in logs and audit rows).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    #[default]
    Api,
    Mcp(String),
    Cli,
}

impl ClientKind {
    /// Actor label used by `search_log.client` and `audit.actor`
    /// (`ui` | `api` | `mcp:<name>` | `cli`).
    pub fn label(&self) -> String {
        match self {
            Self::Ui => "ui".to_string(),
            Self::Api => "api".to_string(),
            Self::Mcp(client) => format!("mcp:{client}"),
            Self::Cli => "cli".to_string(),
        }
    }

    /// Bounded label for metrics (`ui` | `api` | `mcp` | `cli`): the MCP
    /// client name is arbitrary client-supplied text, so `label()` would
    /// make `cauce_search_requests_total{client}` unbounded.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Api => "api",
            Self::Mcp(_) => "mcp",
            Self::Cli => "cli",
        }
    }
}

impl std::fmt::Display for ClientKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// Who originated a stored row (`origin` column / `?origin=` filter,
/// #254): `user` = typed into the UI; `agent` = an answer-loop tool
/// call or any api/mcp/cli client. The stored value is resolved at
/// write time ([`SearchRequest::resolved_origin`]) — a request cannot
/// claim `user` over the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SearchOrigin {
    /// A direct search typed into the web UI (`client = 'ui'` and not
    /// a tool-loop call).
    #[default]
    User,
    /// An answer-loop tool call, or any api/mcp/cli client.
    Agent,
}

impl SearchOrigin {
    /// The `origin` column string (`user` | `agent`).
    pub fn label(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }
}

impl std::fmt::Display for SearchOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for SearchOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            other => Err(format!("unknown origin: {other}")),
        }
    }
}

/// Canonical inbound search request (parent plan section 4.2).
///
/// `client` and missing optionals are filled by the inbound surface, not the
/// caller: `client` defaults to `Api` and is overridden by middleware, `page`
/// defaults to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub q: String,
    #[serde(default = "default_page")]
    pub page: u8,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub time_range: Option<TimeRange>,
    #[serde(default)]
    pub safesearch: SafeSearch,
    /// Explicit engine pin. `None` means "whatever is configured"; `Some` is
    /// part of the cache key, so a pinned search never reuses an unpinned hit.
    #[serde(default)]
    pub engines: Option<Vec<EngineId>>,
    #[serde(default)]
    pub client: ClientKind,
    /// Server-owned provenance marker (#254): `#[serde(skip)]` leaves it
    /// unsettable over the wire (`deny_unknown_fields` rejects the key)
    /// and out of serialized forms — only in-process callers set it.
    /// The answer-loop's `search_web` tool marks its requests `Agent` so
    /// `search_log.origin` can tell tool calls from a human typing in
    /// the UI; every other caller leaves `User` ("direct") and the
    /// pipeline resolves the stored value (see [`Self::resolved_origin`]).
    #[serde(skip)]
    pub origin: SearchOrigin,
}

impl SearchRequest {
    /// The `search_log.origin` this request produces (#254): a
    /// tool-marked request or any non-UI client resolves to `agent`;
    /// only a direct UI search is `user`.
    pub fn resolved_origin(&self) -> SearchOrigin {
        if self.origin == SearchOrigin::Agent || !matches!(self.client, ClientKind::Ui) {
            SearchOrigin::Agent
        } else {
            SearchOrigin::User
        }
    }
}

fn default_page() -> u8 {
    1
}
