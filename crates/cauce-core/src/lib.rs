//! cauce-core: domain types, SearchPipeline, Scheduler, cache tiers, Store and
//! Engine traits, merge/RRF, health. No HTTP server, no SQL; the only I/O
//! dependency is `reqwest` behind the [`http::HttpClient`] engine-egress
//! wrapper (parent plan section 4.1).
//!
//! The shapes here implement section 4.2 of `.agents/plans/2026-09-21-v3-rust-core.md`
//! and are a settled contract: changing them requires amending the parent plan.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod admission;
pub mod ai;
#[cfg(feature = "archive")]
pub mod archive;
mod cache;
pub mod config;
#[cfg(feature = "conformance")]
pub mod conformance;
mod engine;
pub mod evals;
mod health;
pub mod http;
pub mod metrics;
mod normalize;
mod pipeline;
mod request;
mod response;
mod store;

pub use admission::{Admission, AdmissionLimits, FlightResult};
pub use ai::{
    AiCallCtx, AiError, AiStreamEvent, AnswerFrame, AnswerLoop, AnswerRequest, AnthropicClient,
    ChatCompletion, ChatMessage, ChatProvider, ChatRequest, ModelInfo, OpenAiClient, ToolCall,
    ToolSpec, Usage,
};
#[cfg(feature = "archive")]
pub use archive::{ArchiveError, Archiver, MAX_FETCH_BYTES, MAX_MARKDOWN_BYTES};
pub use cache::{CacheKey, CachedSearch, normalize_query};
pub use config::{
    AdmissionConfig, AiConfig, AiProtocol, ArchiveConfig, AuthConfig, CacheConfig, Config,
    ConfigError, Dirs, EgressConfig, EngineEntry, EngineKind, LexicalConfig, LogsConfig,
    MergeConfig, MetaConfig, Resources, SearchConfig, ServerConfig, is_loopback_host,
};
pub use engine::{ENGINE_ID_PATTERN, Engine, EngineError, EngineId, Tier};
pub use evals::{EvalReport, Thresholds};
pub use health::{
    EWMA_ALPHA, EngineHealth, Gate, HealthPolicy, HealthTracker, PERSIST_DEBOUNCE, ProbeGuard,
};
pub use metrics::{EngineMetricStats, EnginePhase, Metrics};
pub use normalize::normalize_url;
pub use pipeline::{
    ArchiveHit, ArchiveSource, CachePolicy, DEFAULT_COLLAPSE_SAME_HOST_AFTER, DEFAULT_DEADLINE,
    DEFAULT_RRF_K, DEFAULT_TTL, DEFAULT_TTL_CAP, HedgePolicy, MergePolicy, PipelineError, RrfMerge,
    SearchOpts, SearchPipeline,
};
pub use request::{ClientKind, SafeSearch, SearchRequest, TimeRange};
pub use response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source, StreamEvent,
    StreamMeta,
};
pub use store::{
    AdmissionStats, AnswerKey, AnswerPayload, AnswerRow, AnswerSource, AuditFacets, AuditFilter,
    AuditRow, BreakerState, CacheResultHit, CacheState, CachedAnswer, ClickRow, ClientCount,
    DayCount, DeleteSearchLog, EngineHealthRow, EngineStatsRow, HistoryFilter, HistoryItem,
    HistoryStats, LatencyPercentiles, LogSource, PAGE_MARK_CLOSE, PAGE_MARK_OPEN, PageHit, PageRow,
    PhaseStats, QueryCount, SearchLogRow, StatsSnapshot, Store, StoreError, StoreTuning, TierHit,
};
