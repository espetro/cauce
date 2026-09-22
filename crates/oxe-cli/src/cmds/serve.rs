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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use oxe_core::config::{Config, EngineEntry, EngineKind, Resources};
use oxe_core::{Engine, SearchPipeline, Tier};
use oxe_engines::exec::{ExecEngine, ExecSpec};
use oxe_engines::replay::Replay;
use oxe_server::{AppState, RouterOptions, observability};
use oxe_store_sqlite::{SqliteStore, spawn_eviction_task};

const USAGE: &str = "usage: oxe serve [--bind ADDR] [--port N] [--headless]";

struct ServeOpts {
    /// `--bind`/`--host` override; else `server.host` (default 127.0.0.1).
    bind: Option<String>,
    /// `--port` override; else `server.port` (default 4479).
    port: Option<u16>,
    /// `--headless`: API + MCP only (wave 0 has no pages yet, so this is
    /// currently equivalent to a full serve).
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
    let code = rt.block_on(serve_async(opts, cfg));
    drop(rt);
    guard.shutdown();
    code
}

async fn serve_async(opts: ServeOpts, cfg: Config) -> i32 {
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
            .with_ttl_cap(Duration::from_secs(cfg.search.ttl_cap_s)),
    );
    let evict = spawn_eviction_task(store.clone());

    let host = opts.bind.clone().unwrap_or_else(|| cfg.server.host.clone());
    let port = opts.port.unwrap_or(cfg.server.port);
    let state = AppState::new(pipeline, store, cfg);
    let app = oxe_server::build_router_opts(state, RouterOptions { ui: !opts.headless });
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

/// Construct the enabled engines (`[[engines]]` entries + `OXE_ENGINES`
/// pinning, resolved by `Config::load`). Wave 0 knows `replay` and `exec`;
/// `declarative` lands in W1 and is skipped with a warning.
fn build_engines(cfg: &Config) -> Vec<Arc<dyn Engine>> {
    let mut out: Vec<Arc<dyn Engine>> = Vec::new();
    for entry in cfg.enabled_engines() {
        match entry.kind {
            EngineKind::Replay => {
                if entry.id.as_str() != "replay" {
                    tracing::warn!(
                        id = %entry.id,
                        "replay engines always run as id \"replay\"; a pin on this id will miss"
                    );
                }
                out.push(Arc::new(Replay::from_env()));
            }
            EngineKind::Exec => {
                let Some(command) = &entry.command else {
                    tracing::warn!(id = %entry.id, "exec engine without command; skipped");
                    continue;
                };
                out.push(Arc::new(ExecEngine::new(ExecSpec {
                    id: entry.id.clone(),
                    command: command.clone(),
                    args: entry.args.clone(),
                    env: entry
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    cwd: entry
                        .cwd
                        .as_ref()
                        .map(PathBuf::from)
                        .or_else(|| resolve_exec_cwd(entry)),
                    page_size: entry.page_size.unwrap_or(10),
                    tier: entry.tier.unwrap_or(Tier::T2),
                })));
            }
            EngineKind::Declarative => {
                tracing::warn!(id = %entry.id, "declarative engines land in W1; skipped");
            }
        }
    }
    out
}

/// `cwd` for an exec entry without one: when the first arg is a relative
/// script path (the `ddgs` built-in ships
/// `sdk/python/oxe_engine_sdk/ddgs_auto.py`), walk up from the process cwd
/// until the file is found so `oxe serve` also works outside the repo root.
fn resolve_exec_cwd(entry: &EngineEntry) -> Option<PathBuf> {
    let script = entry.args.first()?;
    if Path::new(script).is_absolute() {
        return None;
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(script).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
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
