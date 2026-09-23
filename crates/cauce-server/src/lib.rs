//! cauce-server: axum router, HTMX templates, SSE, MCP, Exa adapter, assets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod app;
#[cfg(feature = "ui")]
mod cache_page;
mod error;
mod handlers;
#[cfg(feature = "ui")]
mod html;
#[cfg(feature = "mcp")]
pub mod mcp;
mod metrics;
mod middleware;
pub mod observability;
mod routes;
pub mod strings;

pub use app::{
    AppState, CURRENT_WAVE, RouterOptions, build_router, build_router_opts, feature_enabled,
    mounted_routes, serve,
};
pub use error::ApiError;
pub use metrics::{METRICS_CONTENT_TYPE, MetricsHandle};
pub use middleware::{HostGuard, RequestCtx, host_origin_guard, request_context};
pub use routes::{ROUTES, RouteKind, RouteSpec};
