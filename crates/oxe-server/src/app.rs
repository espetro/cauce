//! Router assembly and the shared [`AppState`].
//!
//! [`build_router`] mounts every [`ROUTES`] row that has a handler arm in
//! [`handler_for`] and is not gated off by [`RouterOptions`]. Mounting is
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
use axum::routing::{MethodRouter, delete, get, post, put};
use axum::{Router, middleware};
use oxe_core::{SearchPipeline, Store, config::Config};
use tokio::net::TcpListener;

use crate::error::ApiError;
use crate::handlers;
use crate::html;
use crate::metrics::MetricsHandle;
use crate::middleware::{HostGuard, RequestCtx, host_origin_guard, request_context};
use crate::routes::{ROUTES, RouteKind, RouteSpec};

/// The wave this build implements; the routes-table test pins
/// `wave <= CURRENT_WAVE` declarations to mounted handlers.
pub const CURRENT_WAVE: u8 = 0;

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

/// Which `requires` features a build mounts. `oxe serve --headless` sets
/// `ui: false` (plan section 6: headless mounts only non-`ui` rows).
#[derive(Debug, Clone)]
pub struct RouterOptions {
    /// Mount `requires: "ui"` rows (the HTMX pages).
    pub ui: bool,
    /// The effective bind host (`--bind` or `server.host`); the Host/Origin
    /// guard (W1-13) accepts it on top of the loopback names.
    pub bind_host: String,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            ui: true,
            bind_host: "127.0.0.1".to_string(),
        }
    }
}

impl RouterOptions {
    /// `oxe serve --headless`: API + MCP, no pages.
    pub fn headless() -> Self {
        Self {
            ui: false,
            ..Self::default()
        }
    }

    fn mounts(&self, spec: &RouteSpec) -> bool {
        match spec.requires {
            Some("ui") => self.ui,
            // Features whose surfaces are not built yet (`mcp` lands in
            // W1-08) mount once a handler arm exists.
            _ => true,
        }
    }
}

/// The [`ROUTES`] rows this router actually mounts: rows with a handler and
/// a satisfied `requires` gate. The routes-table test compares this against
/// the live router.
pub fn mounted_routes(opts: &RouterOptions) -> impl Iterator<Item = &'static RouteSpec> {
    ROUTES
        .iter()
        .filter(|spec| opts.mounts(spec) && handler_for(spec).is_some())
}

/// Mount every declared-and-implemented route and wrap the router in the
/// request-context middleware.
pub fn build_router(state: AppState) -> Router {
    build_router_opts(state, RouterOptions::default())
}

/// [`build_router`] with explicit mount options (`--headless`).
// `wave <= CURRENT_WAVE` reads absurdly while CURRENT_WAVE is 0 (u8 min) but
// is the correct predicate once later waves land: bumping the constant must
// keep earlier-wave routes in the must-implement set, so `==` would be wrong.
#[allow(clippy::absurd_extreme_comparisons)]
pub fn build_router_opts(state: AppState, opts: RouterOptions) -> Router {
    // Hard check: every wave-0 row must be implemented — `requires` is a
    // runtime mount gate (RouterOptions), not a compile-time strip, so a
    // gated row like `GET /` still needs a handler arm. A missing arm is a
    // build-time bug, not a runtime 404.
    for spec in ROUTES.iter().filter(|s| s.wave <= CURRENT_WAVE) {
        assert!(
            handler_for(spec).is_some(),
            "ROUTES: {} {} is wave-{} but has no handler",
            spec.method,
            spec.path,
            spec.wave,
        );
    }

    let mut by_path: BTreeMap<&'static str, MethodRouter<AppState>> = BTreeMap::new();
    for spec in ROUTES.iter().filter(|s| opts.mounts(s)) {
        let Some(mr) = handler_for(spec) else {
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
        .with_state(state)
}

/// The dispatch table: one arm per implemented `(method, path)` pair. A
/// declared row without an arm is the future surface; an arm without a
/// declared row is unreachable (mounting iterates `ROUTES`).
fn handler_for(spec: &RouteSpec) -> Option<MethodRouter<AppState>> {
    match (spec.method, spec.path, spec.kind) {
        ("GET", "/", RouteKind::Html) => Some(get(html::index)),
        ("GET", "/search", RouteKind::Html) => Some(get(html::search)),
        ("GET", "/api/search", RouteKind::Json) => Some(get(handlers::search)),
        ("GET", "/api/history", RouteKind::Json) => Some(get(handlers::history)),
        ("POST", "/api/click", RouteKind::Json) => Some(post(handlers::click)),
        ("GET", "/api/stats", RouteKind::Json) => Some(get(handlers::stats)),
        ("GET", "/api/cache", RouteKind::Json) => Some(get(handlers::cache_list)),
        ("GET", "/api/cache/{key}", RouteKind::Json) => Some(get(handlers::cache_get)),
        ("DELETE", "/api/cache/{key}", RouteKind::Json) => Some(delete(handlers::cache_delete)),
        ("DELETE", "/api/cache", RouteKind::Json) => Some(delete(handlers::cache_bulk_delete)),
        ("GET", "/api/audit", RouteKind::Json) => Some(get(handlers::audit_list)),
        ("GET", "/health", RouteKind::Json) => Some(get(handlers::health)),
        ("GET", "/metrics", RouteKind::Json) => Some(get(handlers::metrics)),
        ("GET", "/api/config", RouteKind::Json) => Some(get(handlers::config_get)),
        ("PUT", "/api/config", RouteKind::Json) => Some(put(handlers::config_put)),
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
