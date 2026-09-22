//! oxe-server: axum router, HTMX templates, SSE, MCP, Exa adapter, assets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod app;
mod error;
mod handlers;
mod middleware;
pub mod observability;
mod routes;

pub use app::{
    AppState, CURRENT_WAVE, RouterOptions, build_router, build_router_opts, mounted_routes, serve,
};
pub use error::ApiError;
pub use middleware::{RequestCtx, request_context};
pub use routes::{ROUTES, RouteKind, RouteSpec};
