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
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use axum::extract::{Extension, Request};
use axum::http::Uri;
#[cfg(feature = "mcp")]
use axum::routing::any_service;
use axum::routing::{MethodRouter, delete, get, post, put};
use axum::{Router, middleware};
use cauce_agent::AnswerLoop;
#[cfg(feature = "archive")]
use cauce_core::Archiver;
use cauce_core::config::{Config, ConfigError};
use cauce_core::{Engine, HealthPolicy, SearchPipeline, Store};
use tokio::net::TcpListener;

#[cfg(feature = "ui")]
use crate::audit_page;
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

/// The runtime `Config` derives: rebuilt wholesale when a config save
/// hot-applies (`commit_config`). Readers clone the `Arc`/struct out
/// under a brief read lock — never held across `.await` — so an in-flight
/// request finishes on the runtime it started with while the next request
/// picks the new values up.
struct Runtime {
    pipeline: Arc<SearchPipeline>,
    /// `Some` only when `[ai]` is `enabled` and the provider client
    /// built; a rejected config logs and degrades to `None`. `POST
    /// /api/answer` answers 503 `ai_disabled` and `/answer` renders its
    /// disabled notice when `None`.
    answer: Option<AnswerLoop>,
    /// The W5-01 fetch-and-index pipeline (`POST /api/pages`, the click
    /// beacon, MCP `fetch_and_index`). `None` when the `archive` feature
    /// is off or the fetcher failed to build; handlers answer 503
    /// `archive_disabled`.
    #[cfg(feature = "archive")]
    archive: Option<Archiver>,
}

/// What [`AppState::commit_config`] applied; handlers translate it into
/// the `applied`/`requires_restart` wire fields.
#[derive(Debug, Clone, Copy)]
pub struct CommitOutcome {
    /// The engine fan-out was rebuilt from the saved config. `false`
    /// means no `engine_factory` is installed — `engines.*` changes are
    /// in the file but keep the boot-time fan-out until restart.
    pub engines_rebuilt: bool,
}

/// Builds the engine fan-out from a `Config` on hot apply — `cauce
/// serve`/`mcp` install `cauce_engines::factory::build_engines` (the
/// server crate has no `cauce-engines` dependency).
type EngineFactory = dyn Fn(&Config) -> Vec<Arc<dyn Engine>> + Send + Sync;

/// Shared handler state: the store, the live config (`commit_config`
/// swaps it under the lock), the derived [`Runtime`] behind a `RwLock`
/// (hot-applied on save), the W1-09 metrics handle (`GET /metrics`
/// scrape off the owned in-process registry) and the engine factory.
#[derive(Clone)]
pub struct AppState {
    runtime: Arc<RwLock<Runtime>>,
    store: Arc<dyn Store>,
    /// `std::sync::Mutex` is deliberate: the critical sections hold a clone,
    /// a file write and a `Config::load()` — sync IO, no `.await` inside.
    config: Arc<Mutex<Config>>,
    /// `None` keeps the boot-time set and reports `engines.*` changes as
    /// restart-required.
    engine_factory: Option<Arc<EngineFactory>>,
    metrics: MetricsHandle,
    /// Process start instant, for the report bundle's `cauce.uptime_s`.
    /// Lives on `AppState` (not the swappable `Runtime`) so a config
    /// hot-apply does not reset the clock.
    started: Instant,
}

impl AppState {
    pub fn new(pipeline: Arc<SearchPipeline>, store: Arc<dyn Store>, config: Config) -> Self {
        rust_i18n::set_locale(&config.ui.locale);
        Self {
            runtime: Arc::new(RwLock::new(Runtime {
                answer: build_answer_loop(&pipeline, &store, &config),
                #[cfg(feature = "archive")]
                archive: build_archive(&store, &config),
                pipeline,
            })),
            metrics: MetricsHandle::new(store.clone()),
            store,
            config: Arc::new(Mutex::new(config)),
            engine_factory: None,
            started: Instant::now(),
        }
    }

    /// Install the factory `commit_config` uses to rebuild the engine
    /// fan-out on save (`cauce_engines::factory::build_engines` in
    /// production). Left unset, `engines.*` changes report as
    /// restart-required.
    pub fn with_engine_factory(
        mut self,
        f: impl Fn(&Config) -> Vec<Arc<dyn Engine>> + Send + Sync + 'static,
    ) -> Self {
        self.engine_factory = Some(Arc::new(f));
        self
    }

    /// The live search pipeline — a shared `Arc`, so the caller can hold
    /// it across `.await` while a config save swaps in a rebuild.
    pub fn pipeline(&self) -> Arc<SearchPipeline> {
        self.runtime
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .pipeline
            .clone()
    }

    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// The process metrics handle (`/metrics` render + cache gauge refresh).
    pub fn metrics(&self) -> &MetricsHandle {
        &self.metrics
    }

    /// Wall-clock uptime since `AppState` was built (`cauce.uptime_s` in
    /// the report bundle).
    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// Run `f` under the config lock. A poisoned lock is recovered (the
    /// config value is still valid; only a panic mid-update poisoned it).
    pub fn with_config<R>(&self, f: impl FnOnce(&mut Config) -> R) -> R {
        let mut guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    /// Save `new_cfg`, swap it into the live config and hot-apply every
    /// non-restart-required change — all under the config lock, so a
    /// concurrent save cannot interleave. The file write goes first: a
    /// failed save leaves the process untouched. Restart-required keys
    /// ([`cauce_core::key_requires_restart`]) are still saved — they take
    /// effect on the next boot and are reported by the caller.
    pub fn commit_config(&self, new_cfg: &Config) -> Result<CommitOutcome, ConfigError> {
        self.with_config(|cfg| self.commit_locked(cfg, new_cfg.clone()))
    }

    /// `commit_config` for callers already inside `with_config`
    /// (`engine_set_enabled` builds its candidate tree there).
    pub(crate) fn commit_locked(
        &self,
        cfg: &mut Config,
        new_cfg: Config,
    ) -> Result<CommitOutcome, ConfigError> {
        new_cfg.save()?;
        let (runtime, engines_rebuilt) = self.build_runtime(&new_cfg);
        rust_i18n::set_locale(&new_cfg.ui.locale);
        *cfg = new_cfg;
        *self.runtime.write().unwrap_or_else(|e| e.into_inner()) = runtime;
        Ok(CommitOutcome { engines_rebuilt })
    }

    /// Rebuild the derived runtime from `cfg` — the engine set via the
    /// factory (the incumbent set when none is installed), a pipeline
    /// reusing the shared [`cauce_core::HealthTracker`] so EWMA/breaker
    /// state survives, a rebuilt answer loop and archiver. Synchronous —
    /// called under the config lock by `commit_locked`.
    fn build_runtime(&self, cfg: &Config) -> (Runtime, bool) {
        let (engines, rebuilt) = match &self.engine_factory {
            Some(factory) => (factory(cfg), true),
            None => (self.pipeline().engines().to_vec(), false),
        };
        let health = self.pipeline().health().clone();
        health.set_policy(HealthPolicy::from_config(&cfg.health));
        let pipeline = Arc::new(
            SearchPipeline::from_config(cfg, self.store.clone(), engines)
                .with_health_tracker(health),
        );
        (
            Runtime {
                answer: build_answer_loop(&pipeline, &self.store, cfg),
                #[cfg(feature = "archive")]
                archive: build_archive(&self.store, cfg),
                pipeline,
            },
            rebuilt,
        )
    }

    /// The grounded-answer loop; `None` when AI answers are off or the
    /// provider client could not be built.
    pub fn answer(&self) -> Option<AnswerLoop> {
        self.runtime
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .answer
            .clone()
    }

    /// The fetch-and-index pipeline; `None` when the feature is off or the
    /// fetcher could not be built.
    #[cfg(feature = "archive")]
    pub fn archive(&self) -> Option<Archiver> {
        self.runtime
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .archive
            .clone()
    }

    /// Whether the results-page click beacon posts indexing requests
    /// (W5-01): `archive.index_on_click` AND a live pipeline. Beacon JS is
    /// only rendered when this is true.
    #[cfg(feature = "archive")]
    pub fn archive_index_on_click(&self) -> bool {
        self.archive().is_some() && self.with_config(|c| c.archive.index_on_click)
    }

    /// Without the `archive` cargo feature there is no beacon.
    #[cfg(not(feature = "archive"))]
    pub fn archive_index_on_click(&self) -> bool {
        false
    }
}

/// Build the grounded-answer loop out of `[ai]` (W4-03): `ai.enabled`
/// plus a successfully constructed provider client (`[ai].protocol`
/// selects OpenAI or Anthropic, W4-05). The client carries `with_audit`
/// so every provider call lands in the audit feed (W4-01 contract).
/// `None` means the `/answer` page renders its disabled notice and
/// `POST /api/answer` rejects as `ai_disabled`.
#[cfg(feature = "ai")]
fn build_answer_loop(
    pipeline: &Arc<SearchPipeline>,
    store: &Arc<dyn Store>,
    config: &Config,
) -> Option<AnswerLoop> {
    if !config.ai.enabled {
        return None;
    }
    match cauce_core::ai::provider_client(&config.ai, Some(store.clone())) {
        Ok(client) => Some(
            AnswerLoop::new(pipeline.as_ref().clone(), client, store.clone())
                // #233: the loop's budget knobs live in `[ai]`; `verify`
                // is reserved for #232 and not yet consulted.
                .with_max_turns(config.ai.max_turns as usize)
                .with_max_search_executions(config.ai.max_searches as usize)
                .with_provider_budget(Duration::from_secs(config.ai.provider_budget_s)),
        ),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "ai.enabled but the provider client failed to build; AI answers disabled"
            );
            None
        }
    }
}

/// Without the `ai` cargo feature the loop can never exist.
#[cfg(not(feature = "ai"))]
fn build_answer_loop(
    pipeline: &Arc<SearchPipeline>,
    store: &Arc<dyn Store>,
    config: &Config,
) -> Option<AnswerLoop> {
    let _ = (pipeline, store, config);
    None
}

/// Build the W5-01 fetch-and-index pipeline from `[archive]`. There is no
/// `archive.enabled` switch — the pipeline exists whenever the feature is
/// compiled; `None` only when the fetcher itself fails to build (a zero
/// politeness knob), logged rather than fatal like `build_answer_loop`.
#[cfg(feature = "archive")]
fn build_archive(store: &Arc<dyn Store>, config: &Config) -> Option<Archiver> {
    match cauce_core::Archiver::new(store.clone(), &config.archive) {
        Ok(archiver) => Some(archiver),
        Err(e) => {
            tracing::warn!(error = %e, "archive fetcher failed to build; page indexing disabled");
            None
        }
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
        // FX-05: `/trace/{id}` is the last HTMX ops page — `/audit`,
        // `/cache`, `/engines`, `/settings`, `/history`, `/dashboard`
        // and `/archive` all live under `/app` now.
        #[cfg(feature = "ui")]
        ("GET", "/trace/{id}", RouteKind::Html) => Some(get(audit_page::trace)),
        #[cfg(feature = "ui")]
        ("GET", "/opensearch.xml", RouteKind::Html) => Some(get(html::opensearch)),
        #[cfg(feature = "ui")]
        ("GET", "/favicon.ico", RouteKind::Static) => Some(get(html::favicon)),
        ("GET", "/api/search", RouteKind::Json) => Some(get(handlers::search)),
        ("GET", "/api/search/stream", RouteKind::Sse) => Some(get(handlers::search_stream)),
        #[cfg(feature = "ai")]
        ("POST", "/api/answer", RouteKind::Sse) => Some(post(handlers::answer)),
        #[cfg(feature = "archive")]
        ("POST", "/api/pages", RouteKind::Json) => Some(post(handlers::pages_index)),
        #[cfg(feature = "archive")]
        ("GET", "/api/pages/{url}", RouteKind::Json) => Some(get(handlers::pages_get)),
        #[cfg(feature = "archive")]
        ("DELETE", "/api/pages/{url}", RouteKind::Json) => Some(delete(handlers::pages_delete)),
        #[cfg(feature = "archive")]
        ("GET", "/api/archive", RouteKind::Json) => Some(get(handlers::archive_search)),
        // `ui` alone: a build without `ai` still mounts `/answer` so the
        // page can render its disabled notice instead of 404ing.
        #[cfg(feature = "ui")]
        ("GET", "/answer", RouteKind::Html) => Some(get(html::answer)),
        // #254: same `ui` gate — the stored render needs no answer loop.
        #[cfg(feature = "ui")]
        ("GET", "/answer/{id}", RouteKind::Html) => Some(get(html::answer_view)),
        // FX-02: the Svelte SPA — shell at `/app`, embedded assets +
        // client-route fallback under `/app/*`. `ui` gate like the pages.
        #[cfg(feature = "ui")]
        ("GET", "/app", RouteKind::Html) => Some(get(html::spa)),
        #[cfg(feature = "ui")]
        ("GET", "/app/{*rest}", RouteKind::Html) => Some(get(html::spa_nested)),
        ("GET", "/api/suggest", RouteKind::Json) => Some(get(handlers::suggest)),
        ("GET", "/api/report", RouteKind::Json) => Some(get(handlers::report)),
        ("GET", "/api/history", RouteKind::Json) => Some(get(handlers::history)),
        ("DELETE", "/api/history/{id}", RouteKind::Json) => Some(delete(handlers::history_delete)),
        ("DELETE", "/api/answer-log/{id}", RouteKind::Json) => {
            Some(delete(handlers::answer_log_delete))
        }
        ("GET", "/api/answer-log/{id}", RouteKind::Json) => Some(get(handlers::answer_log_get)),
        ("POST", "/api/click", RouteKind::Json) => Some(post(handlers::click)),
        ("GET", "/api/stats", RouteKind::Json) => Some(get(handlers::stats)),
        ("GET", "/api/cache", RouteKind::Json) => Some(get(handlers::cache_list)),
        ("GET", "/api/cache/{key}", RouteKind::Json) => Some(get(handlers::cache_get)),
        ("DELETE", "/api/cache/{key}", RouteKind::Json) => Some(delete(handlers::cache_delete)),
        ("DELETE", "/api/cache", RouteKind::Json) => Some(delete(handlers::cache_bulk_delete)),
        ("GET", "/api/audit", RouteKind::Json) => Some(get(handlers::audit_list)),
        ("GET", "/api/engines", RouteKind::Json) => Some(get(handlers::engines_list)),
        ("POST", "/api/engines/{id}/reset", RouteKind::Json) => Some(post(handlers::engine_reset)),
        ("POST", "/api/engines/{id}/enable", RouteKind::Json) => {
            Some(post(handlers::engine_enable))
        }
        ("POST", "/api/engines/{id}/disable", RouteKind::Json) => {
            Some(post(handlers::engine_disable))
        }
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
