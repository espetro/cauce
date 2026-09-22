//! oxe-core: domain types, SearchPipeline, Scheduler, cache tiers, Store and
//! Engine traits, merge/RRF, health. No HTTP, no SQL.
//!
//! The shapes here implement section 4.2 of `.agents/plans/2026-09-21-v3-rust-core.md`
//! and are a settled contract: changing them requires amending the parent plan.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod cache;
pub mod config;
#[cfg(feature = "conformance")]
pub mod conformance;
mod engine;
mod normalize;
mod pipeline;
mod request;
mod response;
mod store;

pub use cache::{CacheKey, CachedSearch, normalize_query};
pub use config::{
    AiConfig, CacheConfig, Config, ConfigError, Dirs, EngineEntry, EngineKind, LexicalConfig,
    LogsConfig, MetaConfig, Resources, SearchConfig, ServerConfig,
};
pub use engine::{Engine, EngineError, EngineId, Tier};
pub use normalize::normalize_url;
pub use pipeline::{
    DEFAULT_DEADLINE, DEFAULT_TTL, DEFAULT_TTL_CAP, PipelineError, SearchOpts, SearchPipeline,
};
pub use request::{ClientKind, SafeSearch, SearchRequest, TimeRange};
pub use response::{EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source};
pub use store::{
    AuditFilter, AuditRow, BreakerState, ClickRow, ClientCount, DayCount, EngineHealthRow,
    HistoryFilter, HistoryItem, LatencyPercentiles, LogSource, SearchLogRow, StatsSnapshot, Store,
    StoreError, StoreTuning,
};
