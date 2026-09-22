//! `oxe serve [--bind ADDR] [--port N] [--headless]`: run the HTTP server.
//!
//! Wiring order: `Config::load` (env overrides included) -> install
//! observability (JSONL logs under `logs/`) -> `SqliteStore` under
//! `Resources::detect()` tuning -> engine factory over `enabled_engines` ->
//! `SearchPipeline` with `search.*` tunables -> axum router -> serve loop.
//! The `ObservabilityGuard` is held for the whole process lifetime and
//! flushed on shutdown.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use oxe_core::config::{Config, Resources, is_loopback_host};
use oxe_core::{Admission, AdmissionLimits, SearchPipeline};
use oxe_engines::factory::build_engines;
use oxe_server::{AppState, RouterOptions, observability};
use oxe_store_sqlite::{SqliteStore, spawn_eviction_task};

const USAGE: &str = "usage: oxe serve [--bind ADDR] [--port N] [--headless]";

struct ServeOpts {
    /// `--bind`/`--host` override; else `server.host` (default 127.0.0.1).
    bind: Option<String>,
    /// `--port` override; else `server.port` (default 4479).
    port: Option<u16>,
    /// `--headless`: API only; the `requires = "ui"` routes (`/`, `/search`
    /// HTMX pages) stay unmounted.
    headless: bool,
}

/// Entry point for the `serve` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("oxe serve: {msg}\n{USAGE}");
            return 2;
        }
    };
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("oxe serve: {e}");
            return 2;
        }
    };
    // W1-13: `auth.enabled` is forced when the bind is not loopback, but
    // admin auth itself is deferred (v3/later/postgres-and-multi-instance.md),
    // so a non-loopback bind refuses to start rather than listen
    // unauthenticated. Loopback requests are still guarded by the
    // Host/Origin check in oxe-server.
    let host = opts.bind.clone().unwrap_or_else(|| cfg.server.host.clone());
    if cfg.auth.enabled_for(&host) {
        if is_loopback_host(&host) {
            eprintln!(
                "oxe serve: warning: auth.enabled = true but admin auth is not implemented yet; ignoring"
            );
        } else {
            eprintln!(
                "oxe serve: refusing to bind {host}: non-loopback listen requires auth.enabled, \
                 but admin auth is not implemented yet"
            );
            return 1;
        }
    }
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("oxe serve: cannot create data dirs: {e}");
        return 1;
    }
    let obs = observability::ObservabilityConfig {
        logs_dir: cfg.logs_dir(),
        retention_days: cfg.logs.retention_days as usize,
        ..Default::default()
    };
    let guard = match observability::init(&obs) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("oxe serve: logging init failed: {e}");
            return 1;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("oxe serve: tokio runtime: {e}");
            return 1;
        }
    };
    let code = rt.block_on(serve_async(opts, cfg, host));
    drop(rt);
    guard.shutdown();
    code
}

async fn serve_async(opts: ServeOpts, cfg: Config, host: String) -> i32 {
    let tuning = Resources::detect().store_tuning;
    let store = match SqliteStore::open(cfg.db_path(), tuning) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::error!(error = %e, path = %cfg.db_path().display(), "cannot open store");
            return 1;
        }
    };
    let engines = build_engines(&cfg);
    if engines.is_empty() {
        tracing::warn!(
            "no engines enabled (see [[engines]] / OXE_ENGINES); /api/search will answer 503"
        );
    }
    let pipeline = Arc::new(
        SearchPipeline::new(store.clone(), engines)
            .with_deadline(Duration::from_millis(cfg.search.deadline_ms))
            .with_default_ttl(Duration::from_secs(cfg.search.ttl_s))
            .with_ttl_cap(Duration::from_secs(cfg.search.ttl_cap_s))
            .with_lexical(cfg.cache.lexical)
            .with_admission(Admission::new(AdmissionLimits {
                max_wait: Duration::from_millis(cfg.admission.max_wait_ms),
                max_concurrent_per_engine: cfg.admission.max_concurrent_per_engine.max(1) as usize,
            })),
    );
    let evict = spawn_eviction_task(store.clone());

    let port = opts.port.unwrap_or(cfg.server.port);
    let state = AppState::new(pipeline, store, cfg);
    let app = oxe_server::build_router_opts(
        state,
        RouterOptions {
            ui: !opts.headless,
            bind_host: host.clone(),
        },
    );
    let listener = match tokio::net::TcpListener::bind((host.as_str(), port)).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(error = %e, host, port, "bind failed");
            evict.abort();
            return 1;
        }
    };
    match listener.local_addr() {
        Ok(addr) => tracing::info!(%addr, headless = opts.headless, "oxe serve listening"),
        Err(e) => tracing::warn!(error = %e, "bound but local_addr failed"),
    }
    let result = oxe_server::serve(listener, app).await;
    evict.abort();
    match result {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, "serve failed");
            1
        }
    }
}

fn parse(args: &[String]) -> Result<ServeOpts, String> {
    let mut opts = ServeOpts {
        bind: None,
        port: None,
        headless: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("missing value for {name}"))
        };
        match flag {
            "--bind" | "--host" => opts.bind = Some(value(flag)?),
            "--port" | "-p" => {
                let raw = value(flag)?;
                opts.port = Some(
                    raw.parse::<u16>()
                        .map_err(|_| format!("invalid --port {raw:?}"))?,
                );
            }
            "--headless" => opts.headless = true,
            "-h" | "--help" => return Err("help requested".into()),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    Ok(opts)
}
