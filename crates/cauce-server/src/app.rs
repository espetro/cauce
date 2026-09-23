//! Router assembly and the shared [`AppState`].
//!
//! [`build_router`] mounts every [`ROUTES`] row that has a handler arm in
//! [`handler_for`], whose `requires` cargo feature is compiled in, and that
//! is not gated off by [`RouterOptions`]. Mounting is
//! driven by iterating `ROUTES`, never by a separate route list, so an
//! undeclared path cannot be mounted and a declared-but-handlerless wave-0
//! row panics the build instead of silently 404ing.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Extension, Request};
use axum::http::Uri;
#[cfg(feature = "mcp")]
use axum::routing::any_service;
use axum::routing::{MethodRouter, delete, get, post, put};
use axum::{Router, middleware};
use cauce_core::{SearchPipeline, Store, config::Config};
use tokio::net::TcpListener;

#[cfg(feature = "ui")]
use crate::audit_page;
#[cfg(feature = "ui")]
use crate::cache_page;
#[cfg(feature = "ui")]
use crate::dashboard;
use crate::error::ApiError;
use crate::handlers;
#[cfg(feature = "ui")]
use crate::html;
#[cfg(feature = "mcp")]
use crate::mcp;
use crate::metrics::MetricsHandle;
use crate::middleware::{HostGuard, RequestCtx, host_origin_guard, request_context};
use crate::routes::{ROUTES, RouteKind, RouteSpec};

/// The wave this build implements; the routes-table test pins
/// `wave <= CURRENT_WAVE` declarations to mounted handlers.
pub const CURRENT_WAVE: u8 = 1;

/// Shared handler state: the search pipeline, the store, the live config
/// (`PUT /api/config` swaps it under the lock) and the W1-09 metrics
/// handle (`GET /metrics` scrape off the owned in-process registry).
#[derive(Clone)]
pub struct AppState {
    pipeline: Arc<SearchPipeline>,
    store: Arc<dyn Store>,
    /// `std::sync::Mutex` is deliberate: the critical sections hold a clone,
    /// a file write and a `Config::load()` — sync IO, no `.await` inside.
    config: Arc<Mutex<Config>>,
    metrics: MetricsHandle,
}

impl AppState {
    pub fn new(pipeline: Arc<SearchPipeline>, store: Arc<dyn Store>, config: Config) -> Self {
        Self {
            pipeline,
            metrics: MetricsHandle::new(store.clone()),
            store,
            config: Arc::new(Mutex::new(config)),
        }
    }

    pub fn pipeline(&self) -> &SearchPipeline {
        &self.pipeline
    }

    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// The process metrics handle (`/metrics` render + cache gauge refresh).
    pub fn metrics(&self) -> &MetricsHandle {
        &self.metrics
    }

    /// Run `f` under the config lock. A poisoned lock is recovered (the
    /// config value is still valid; only a panic mid-update poisoned it).
    pub fn with_config<R>(&self, f: impl FnOnce(&mut Config) -> R) -> R {
        let mut guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}

/// Which `requires` features a build mounts. `cauce serve --headless` sets
/// `ui: false` (plan section 6: headless mounts only non-`ui` rows).
#[derive(Debug, Clone)]
pub struct RouterOptions {
    /// Mount `requires: "ui"` rows (the HTMX pages).
    pub ui: bool,
    /// The effective bind host (`--bind` or `server.host`); the Host/Origin
    /// guard (W1-13) accepts it on top of the loopback names.
    pub bind_host: String,
    /// Effective listener port (`--port` or `server.port`) for safe URL fallback.
    pub bind_port: u16,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            ui: true,
            bind_host: "127.0.0.1".to_string(),
            bind_port: 4479,
        }
    }
}

impl RouterOptions {
    /// `cauce serve --headless`: API + MCP, no pages.
    pub fn headless() -> Self {
        Self {
            ui: false,
            ..Self::default()
        }
    }

    /// Whether `spec` mounts under these options: the `requires` cargo
    /// feature must be compiled in (see [`feature_enabled`]) and, for
    /// `requires: "ui"`, the runtime `--headless` gate must allow pages.
    fn mounts(&self, spec: &RouteSpec) -> bool {
        match spec.requires {
            Some(req) if !feature_enabled(req) => false,
            Some("ui") => self.ui,
            Some(_) | None => true,
        }
    }
}

/// Whether the cargo feature `name` (a [`RouteSpec::requires`] value) is
/// compiled into this build. Unknown names never mount — a typo'd
/// `requires` fails closed rather than silently mounting the row.
pub fn feature_enabled(name: &str) -> bool {
    match name {
        "ui" => cfg!(feature = "ui"),
        "mcp" => cfg!(feature = "mcp"),
        "ai" => cfg!(feature = "ai"),
        "archive" => cfg!(feature = "archive"),
        "semantic" => cfg!(feature = "semantic"),
        "postgres" => cfg!(feature = "postgres"),
        "otlp" => cfg!(feature = "otlp"),
        _ => false,
    }
}

/// The [`ROUTES`] rows this router actually mounts: rows with a handler and
/// a satisfied `requires` gate. The routes-table test compares this against
/// the live router.
pub fn mounted_routes<'a>(
    state: &'a AppState,
    opts: &'a RouterOptions,
) -> impl Iterator<Item = &'static RouteSpec> + 'a {
    ROUTES
        .iter()
        .filter(move |spec| opts.mounts(spec) && handler_for(spec, state).is_some())
}

/// Mount every declared-and-implemented route and wrap the router in the
/// request-context middleware.
pub fn build_router(state: AppState) -> Router {
    build_router_opts(state, RouterOptions::default())
}

/// [`build_router`] with explicit mount options (`--headless`).
// `wave <= CURRENT_WAVE` is deliberately `<=`, not `==`: bumping the
// constant must keep earlier-wave routes in the must-implement set.
pub fn build_router_opts(state: AppState, opts: RouterOptions) -> Router {
    // Hard check: every wave <= CURRENT_WAVE row whose `requires` feature is
    // compiled in must have a handler arm — a missing arm is a build-time
    // bug, not a runtime 404. Rows gated behind a feature this build lacks
    // (e.g. `GET /` in a `--no-default-features` build) are legitimately
    // arm-less and stay unmounted.
    for spec in ROUTES
        .iter()
        .filter(|s| s.wave <= CURRENT_WAVE && s.requires.is_none_or(feature_enabled))
    {
        assert!(
            handler_for(spec, &state).is_some(),
            "ROUTES: {} {} is wave-{} but has no handler",
            spec.method,
            spec.path,
            spec.wave,
        );
    }

    let mut by_path: BTreeMap<&'static str, MethodRouter<AppState>> = BTreeMap::new();
    for spec in ROUTES.iter().filter(|s| opts.mounts(s)) {
        let Some(mr) = handler_for(spec, &state) else {
            continue;
        };
        // `MethodRouter::merge` takes `self` by value; `mem::take` leaves a
        // default in place while the merged router is rebuilt.
        let slot = by_path.entry(spec.path).or_default();
        *slot = std::mem::take(slot).merge(mr);
    }

    let mut router = Router::new();
    for (path, mr) in by_path {
        router = router.route(path, mr);
    }
    router
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        // The Host/Origin guard (W1-13) runs inside `request_context`, so
        // a rejected request still carries `X-Request-Id` and its span.
        .layer(middleware::from_fn_with_state(
            HostGuard::new(&opts.bind_host),
            host_origin_guard,
        ))
        .layer(middleware::from_fn(request_context))
        .layer(Extension(opts))
        .with_state(state)
}

/// The dispatch table: one arm per implemented `(method, path)` pair. A
/// declared row without an arm is the future surface; an arm without a
/// declared row is unreachable (mounting iterates `ROUTES`).
///
/// Takes `state` because some handlers are built from it (the MCP streamable
/// service captures the shared pipeline/store) rather than extracting it via
/// `State<AppState>` at request time. `state` is only read by the `/mcp`
/// arm today, so it is unused in non-`mcp` builds.
#[cfg_attr(not(feature = "mcp"), allow(unused_variables))]
fn handler_for(spec: &RouteSpec, state: &AppState) -> Option<MethodRouter<AppState>> {
    match (spec.method, spec.path, spec.kind) {
        #[cfg(feature = "ui")]
        ("GET", "/", RouteKind::Html) => Some(get(html::index)),
        #[cfg(feature = "ui")]
        ("GET", "/search", RouteKind::Html) => Some(get(html::search)),
        #[cfg(feature = "ui")]
        ("GET", "/audit", RouteKind::Html) => Some(get(audit_page::audit)),
        #[cfg(feature = "ui")]
        ("GET", "/trace/{id}", RouteKind::Html) => Some(get(audit_page::trace)),
        #[cfg(feature = "ui")]
        ("GET", "/settings", RouteKind::Html) => Some(get(html::settings)),
        #[cfg(feature = "ui")]
        ("GET", "/cache", RouteKind::Html) => Some(get(cache_page::cache)),
        #[cfg(feature = "ui")]
        ("GET", "/opensearch.xml", RouteKind::Html) => Some(get(html::opensearch)),
        #[cfg(feature = "ui")]
        ("GET", "/favicon.ico", RouteKind::Static) => Some(get(html::favicon)),
        #[cfg(feature = "ui")]
        ("GET", "/dashboard", RouteKind::Html) => Some(get(dashboard::dashboard)),
        ("GET", "/api/search", RouteKind::Json) => Some(get(handlers::search)),
        ("GET", "/api/history", RouteKind::Json) => Some(get(handlers::history)),
        ("POST", "/api/click", RouteKind::Json) => Some(post(handlers::click)),
        ("GET", "/api/stats", RouteKind::Json) => Some(get(handlers::stats)),
        ("GET", "/api/cache", RouteKind::Json) => Some(get(handlers::cache_list)),
        ("GET", "/api/cache/{key}", RouteKind::Json) => Some(get(handlers::cache_get)),
        ("DELETE", "/api/cache/{key}", RouteKind::Json) => Some(delete(handlers::cache_delete)),
        ("DELETE", "/api/cache", RouteKind::Json) => Some(delete(handlers::cache_bulk_delete)),
        ("GET", "/api/audit", RouteKind::Json) => Some(get(handlers::audit_list)),
        ("GET", "/api/engines", RouteKind::Json) => Some(get(handlers::engines_list)),
        ("POST", "/api/engines/{id}/reset", RouteKind::Json) => Some(post(handlers::engine_reset)),
        ("GET", "/health", RouteKind::Json) => Some(get(handlers::health)),
        ("GET", "/metrics", RouteKind::Json) => Some(get(handlers::metrics)),
        ("GET", "/api/config", RouteKind::Json) => Some(get(handlers::config_get)),
        ("PUT", "/api/config", RouteKind::Json) => Some(put(handlers::config_put)),
        // The MCP streamable-HTTP transport is a `tower::Service` serving
        // every method on `/mcp` (W1-08): `any_service` mounts it directly.
        #[cfg(feature = "mcp")]
        ("*", "/mcp", RouteKind::Mcp) => Some(any_service(mcp::streamable_service(state.clone()))),
        _ => None,
    }
}

/// 404 fallback: the error envelope applies to unmatched paths too.
async fn not_found(uri: Uri, ctx: Option<Extension<RequestCtx>>) -> ApiError {
    ApiError::not_found(format!("no route for {}", uri.path()))
        .with_request_id(ctx.as_ref().map(|Extension(c)| c.request_id.as_uuid()))
}

/// 405 fallback: a declared path hit with the wrong method.
async fn method_not_allowed(request: Request) -> ApiError {
    let request_id = request
        .extensions()
        .get::<RequestCtx>()
        .map(|c| c.request_id.as_uuid());
    ApiError::method_not_allowed(format!(
        "method {} not allowed for {}",
        request.method(),
        request.uri().path()
    ))
    .with_request_id(request_id)
}

/// Serve `listener` until SIGINT/SIGTERM, then finish in-flight requests.
///
/// Callers build the router themselves (`build_router_opts`), so
/// `--headless` and test wiring stay outside this function.
pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM; only Ctrl-C stops the server");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
