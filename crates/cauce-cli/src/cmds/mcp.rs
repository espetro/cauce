//! `cauce mcp`: the stdio MCP transport (W1-08) — same config, same DB, same
//! engines and pipeline as `cauce serve`, but no HTTP listener. stdout carries
//! JSON-RPC frames exclusively; all logs go to the JSONL files (and stderr
//! when pretty logging is on) so a transport peer never sees a stray line.
//!
//! Wiring order mirrors `cmds::serve`: `Config::load` -> observability ->
//! `SqliteStore` -> engine factory -> `SearchPipeline` -> `AppState` ->
//! `cauce_server::mcp::serve_stdio`, which runs until the client closes stdin.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use cauce_core::config::{Config, Resources};
use cauce_core::{Admission, AdmissionLimits, HedgePolicy, SearchPipeline};
use cauce_engines::factory::build_engines;
use cauce_server::{AppState, observability};
use cauce_store_sqlite::{SqliteStore, spawn_eviction_task};

const USAGE: &str = "usage: cauce mcp";

/// Entry point for the `mcp` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    // `cauce mcp` takes no flags; anything past the subcommand is an error
    // (except `--help`).
    if let Some(arg) = args.first() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "{USAGE}\n\nServe the cauce MCP tools over stdio (no HTTP listener).\nTools: search_web, cache_status, cache_invalidate, exa_search."
                );
                return 0;
            }
            other => {
                eprintln!("cauce mcp: unknown flag {other:?}\n{USAGE}");
                return 2;
            }
        }
    }
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("cauce mcp: {e}");
            return 2;
        }
    };
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("cauce mcp: cannot create data dirs: {e}");
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
            eprintln!("cauce mcp: logging init failed: {e}");
            return 1;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cauce mcp: tokio runtime: {e}");
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
        tracing::warn!(
            "no engines enabled (see [[engines]] / CAUCE_ENGINES); search_web will fail"
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
            }))
            .with_hedge(HedgePolicy {
                floor: Duration::from_millis(cfg.search.hedge_floor_ms),
                ceiling: Duration::from_millis(cfg.search.hedge_ceiling_ms),
                min_results: cfg.search.min_results as usize,
            }),
    );
    // Restore persisted breakers, same as `cauce serve`: a stdio process
    // must respect a breaker `serve` opened (parallel agents share the
    // egress IP) and its own writes must not clobber `serve`'s rows with
    // stale all-Closed state.
    match pipeline.load_health().await {
        Ok(n) if n > 0 => tracing::info!(rows = n, "engine health restored"),
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "engine health load failed; starting with closed breakers")
        }
    }
    let evict = spawn_eviction_task(store.clone());
    let state = AppState::new(pipeline.clone(), store, cfg);
    tracing::info!("cauce mcp serving stdio");
    let result = cauce_server::mcp::serve_stdio(state).await;
    evict.abort();
    // Best-effort final flush, same as `cauce serve`.
    if let Err(e) = pipeline.health().flush().await {
        tracing::warn!(error = %e, "engine health flush on shutdown failed");
    }
    match result {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, "stdio transport failed");
            1
        }
    }
}
