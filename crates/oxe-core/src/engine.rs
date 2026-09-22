//! Engine contract: `EngineId`, `Tier`, `EngineError`, and the `Engine` trait.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::request::SearchRequest;
use crate::response::SearchResult;

/// Stable identifier of a search engine (`bing`, `brave`, `ddgs`, `replay`, ...).
///
/// Serializes as a plain string on the wire and in `engines_json` columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineId(String);

impl EngineId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EngineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EngineId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<&String> for EngineId {
    fn from(s: &String) -> Self {
        Self(s.clone())
    }
}

impl From<String> for EngineId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Fan-out tier of an engine (parent plan section 4.3): 1 fast/reliable,
/// 2 hedge, 3 specialised. Serializes as its integer on the wire and in YAML
/// engine specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    T1 = 1,
    T2 = 2,
    T3 = 3,
}

impl Tier {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for Tier {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(Self::T1),
            2 => Ok(Self::T2),
            3 => Ok(Self::T3),
            other => Err(format!("invalid engine tier: {other}")),
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

impl Serialize for Tier {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for Tier {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(d)?;
        Self::try_from(v).map_err(serde::de::Error::custom)
    }
}

/// Errors an engine can return. Variants carrying a `String` hold a short,
/// log-safe detail (no response bodies).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum EngineError {
    #[error("rate limited by upstream")]
    RateLimited,
    #[error("blocked by upstream (captcha)")]
    Blocked,
    #[error("timed out")]
    Timeout,
    #[error("parse error: {0}")]
    Parse(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("no results")]
    NoResults,
}

/// A search engine (parent plan section 4.2). Implementations live in
/// `oxe-engines`; this crate only defines the contract.
#[async_trait]
pub trait Engine: Send + Sync {
    fn id(&self) -> EngineId;
    fn tier(&self) -> Tier;
    fn page_size(&self) -> u8;

    /// Run one page of `req` within `budget`. Implementations must respect the
    /// deadline: results arriving after it are wasted work.
    async fn search(
        &self,
        req: &SearchRequest,
        budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError>;
}
