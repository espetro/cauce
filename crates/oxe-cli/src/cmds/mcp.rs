//! `oxe mcp`: the stdio MCP transport (W1-08) — same config, same DB, same
//! engines and pipeline as `oxe serve`, but no HTTP listener. stdout carries
//! JSON-RPC frames exclusively; all logs go to the JSONL files (and stderr
//! when pretty logging is on) so a transport peer never sees a stray line.
//!
//! Wiring order mirrors `cmds::serve`: `Config::load` -> observability ->
//! `SqliteStore` -> engine factory -> `SearchPipeline` -> `AppState` ->
//! `oxe_server::mcp::serve_stdio`, which runs until the client closes stdin.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use oxe_core::SearchPipeline;
use oxe_core::config::{Config, Resources};
use oxe_engines::factory::build_engines;
use oxe_server::{AppState, observability};
use oxe_store_sqlite::{SqliteStore, spawn_eviction_task};

const USAGE: &str = "usage: oxe mcp";

/// Entry point for the `mcp` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    // `oxe mcp` takes no flags; anything past the subcommand is an error
    // (except `--help`).
    if let Some(arg) = args.first() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "{USAGE}\n\nServe the oxe MCP tools over stdio (no HTTP listener).\nTools: search_web, cache_status, cache_invalidate, exa_search."
                );
                return 0;
            }
            other => {
                eprintln!("oxe mcp: unknown flag {other:?}\n{USAGE}");
                return 2;
            }
        }
    }
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("oxe mcp: {e}");
            return 2;
        }
    };
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("oxe mcp: cannot create data dirs: {e}");
        return 1;
    }
    let obs = observability::ObservabilityConfig {
        logs_dir: cfg.logs_dir(),
        retention_days: cfg.logs.retention_days as usize,
        // stdout is the protocol channel; the pretty layer writes stderr
        // only, so it stays safe to leave enabled.
        ..Default::default()
    };
    let guard = match observability::init(&obs) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("oxe mcp: logging init failed: {e}");
            return 1;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("oxe mcp: tokio runtime: {e}");
            return 1;
        }
    };
    let code = rt.block_on(mcp_async(cfg));
    drop(rt);
    guard.shutdown();
    code
}

async fn mcp_async(cfg: Config) -> i32 {
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
        tracing::warn!("no engines enabled (see [[engines]] / OXE_ENGINES); search_web will fail");
    }
    let pipeline = Arc::new(
        SearchPipeline::new(store.clone(), engines)
            .with_deadline(Duration::from_millis(cfg.search.deadline_ms))
            .with_default_ttl(Duration::from_secs(cfg.search.ttl_s))
            .with_ttl_cap(Duration::from_secs(cfg.search.ttl_cap_s)),
    );
    let evict = spawn_eviction_task(store.clone());
    let state = AppState::new(pipeline, store, cfg);
    tracing::info!("oxe mcp serving stdio");
    let result = oxe_server::mcp::serve_stdio(state).await;
    evict.abort();
    match result {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, "stdio transport failed");
            1
        }
    }
}
