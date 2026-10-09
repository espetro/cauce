//! cauce-server: axum router, embedded Svelte SPA, SSE, MCP, Exa adapter, assets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Compile-time-embedded string catalog (`locales/*.yaml`); `t!` resolves
// from it, `[ui].locale` picks the language at startup (see `i18n.rs`).
rust_i18n::i18n!("locales", fallback = "en");

mod app;
#[cfg(feature = "ui")]
mod assets;

mod capabilities;
mod error;
mod handlers;
pub mod i18n;
#[cfg(feature = "mcp")]
pub mod mcp;
mod metrics;
mod middleware;
pub mod observability;
#[cfg(feature = "ui")]
mod pages;
pub mod report;
mod routes;
mod settings;
#[cfg(feature = "ui")]
mod spa;

pub use app::{
    AppState, CURRENT_WAVE, RouterOptions, build_router, build_router_opts, feature_enabled,
    mounted_routes, serve,
};
pub use capabilities::{Capabilities, CapabilityFlags, InstanceInfo, InstanceMode, Role};
pub use error::ApiError;
#[cfg(feature = "ai")]
pub use handlers::AnswerBody;
pub use handlers::{
    AnswerLogDeleteAck, CacheBulkDeleteAck, CacheDeleteAck, CacheListing, ConfigPutResponse,
    EngineToggleAck, EngineView, HistoryDeleteAck, SuggestResponse,
};
#[cfg(feature = "archive")]
pub use handlers::{ArchiveResponse, ArchiveRow, IndexBody, PageDeleteAck};
pub use metrics::{METRICS_CONTENT_TYPE, MetricsHandle};
pub use middleware::{HostGuard, RequestCtx, host_origin_guard, request_context, require_admin};
pub use routes::{ROUTES, RouteAuth, RouteKind, RouteSpec};
