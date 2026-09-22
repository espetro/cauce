//! `cauce search "<q>" [--json|--table|--urls] [--engines a[,b]...]`
//! (W2-11): run the search pipeline in-process — no server needed — against
//! the same config and SQLite DB `cauce serve` uses.
//!
//! Wiring mirrors `cmds::serve`: `Config::load` -> data dirs ->
//! observability (JSONL, so `cauce trace <request_id>` rebuilds the CLI
//! fan-out too) -> `SqliteStore` -> engine factory -> `SearchPipeline`
//! with `search.*`/`admission.*` tunables -> persisted breakers loaded
//! before the search and flushed after. The request runs as
//! `ClientKind::Cli` and the `request_id` is printed to stderr so stdout
//! stays pipe-clean.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use cauce_core::config::{Config, Resources};
use cauce_core::{
    Admission, AdmissionLimits, ClientKind, EngineId, SafeSearch, SearchPipeline, SearchRequest,
    SearchResponse,
};
use cauce_engines::factory::build_engines;
use cauce_server::observability;
use cauce_store_sqlite::SqliteStore;

const USAGE: &str = "usage: cauce search \"<q>\" [--json|--table|--urls] [--engines <a[,b]...>]";

/// Output format selected by the format flags. `Table` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Table,
    Json,
    Urls,
}

struct SearchArgs {
    q: String,
    format: Format,
    engines: Option<Vec<EngineId>>,
}

/// Entry point for the `search` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("cauce search: {msg}\n{USAGE}");
            return 2;
        }
    };
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("cauce search: {e}");
            return 2;
        }
    };
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("cauce search: cannot create data dirs: {e}");
        return 1;
    }
    // JSONL logging is what makes a CLI search traceable by request_id;
    // a logging failure must not stop a one-shot search from running.
    let guard = match observability::init(&observability::ObservabilityConfig {
        logs_dir: cfg.logs_dir(),
        retention_days: cfg.logs.retention_days as usize,
        ..Default::default()
    }) {
        Ok(guard) => Some(guard),
        Err(e) => {
            eprintln!("cauce search: logging init failed ({e}); continuing without JSONL logs");
            None
        }
    };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cauce search: tokio runtime: {e}");
            return 1;
        }
    };
    let code = rt.block_on(search_async(&cfg, &opts));
    drop(rt);
    if let Some(guard) = guard {
        guard.shutdown();
    }
    code
}

async fn search_async(cfg: &Config, opts: &SearchArgs) -> i32 {
    let tuning = Resources::detect().store_tuning;
    let store = match SqliteStore::open(cfg.db_path(), tuning) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            eprintln!(
                "cauce search: cannot open store {}: {e}",
                cfg.db_path().display()
            );
            return 1;
        }
    };
    let engines = build_engines(cfg);
    if engines.is_empty() {
        eprintln!("cauce search: no engines enabled (see [[engines]] / CAUCE_ENGINES)");
        return 1;
    }
    let pipeline = SearchPipeline::new(store, engines)
        .with_deadline(Duration::from_millis(cfg.search.deadline_ms))
        .with_default_ttl(Duration::from_secs(cfg.search.ttl_s))
        .with_ttl_cap(Duration::from_secs(cfg.search.ttl_cap_s))
        .with_lexical(cfg.cache.lexical)
        .with_admission(Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(cfg.admission.max_wait_ms),
            max_concurrent_per_engine: cfg.admission.max_concurrent_per_engine.max(1) as usize,
        }));
    // Honour persisted breakers (plan 4.4.6), same as `serve` startup.
    if let Err(e) = pipeline.load_health().await {
        tracing::warn!(error = %e, "engine health load failed; starting with closed breakers");
    }

    let req = SearchRequest {
        q: opts.q.clone(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: opts.engines.clone(),
        client: ClientKind::Cli,
    };
    // Mint the id here so it is printed even when the pipeline errors —
    // the search_log row and JSONL spans carry it either way.
    let request_id = uuid::Uuid::now_v7();
    let result = pipeline.search_with_id(&req, request_id).await;
    eprintln!("request_id: {request_id}");
    let code = match result {
        Ok(resp) => {
            print_response(&resp, opts.format);
            0
        }
        Err(e) => {
            eprintln!("cauce search: {e}");
            1
        }
    };
    // Persist EWMA/failure updates made by this search.
    if let Err(e) = pipeline.health().flush().await {
        tracing::warn!(error = %e, "engine health flush failed");
    }
    code
}

fn print_response(resp: &SearchResponse, format: Format) {
    match format {
        Format::Json => match serde_json::to_string_pretty(resp) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("cauce search: serialising response: {e}"),
        },
        Format::Urls => {
            for r in &resp.results {
                println!("{}", r.url);
            }
        }
        Format::Table => print_table(resp),
    }
    eprintln!(
        "{} · {} results · {} ms",
        source_label(resp),
        resp.results.len(),
        resp.meta.elapsed_ms
    );
}

fn print_table(resp: &SearchResponse) {
    println!("{:<3} {:<50} {:<12} URL", "#", "TITLE", "ENGINE");
    for (i, r) in resp.results.iter().enumerate() {
        println!(
            "{:<3} {:<50} {:<12} {}",
            i + 1,
            truncate(&r.title, 50),
            r.engine,
            r.url
        );
    }
}

fn source_label(resp: &SearchResponse) -> String {
    match &resp.meta.source {
        cauce_core::Source::Cache { tier, age_s, .. } => {
            format!("cache tier {} · {} s ago", tier.as_u8(), age_s)
        }
        cauce_core::Source::Network => "live".to_string(),
    }
}

/// Truncate to `max` chars, adding an ellipsis when cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn parse(args: &[String]) -> Result<SearchArgs, String> {
    let mut q = None;
    let mut format = None;
    let mut engines: Vec<EngineId> = Vec::new();
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
            "--json" => set_format(&mut format, Format::Json)?,
            "--table" => set_format(&mut format, Format::Table)?,
            "--urls" => set_format(&mut format, Format::Urls)?,
            "--engines" => {
                let v = value("--engines")?;
                for id in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    engines.push(EngineId::from(id));
                }
            }
            "-h" | "--help" => return Err("help requested".into()),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag {other:?}"));
            }
            positional => {
                if q.replace(positional.to_string()).is_some() {
                    return Err("only one <q> positional allowed".to_string());
                }
            }
        }
    }
    let q = q.filter(|q| !q.trim().is_empty()).ok_or("missing <q>")?;
    Ok(SearchArgs {
        q,
        format: format.unwrap_or(Format::Table),
        engines: (!engines.is_empty()).then_some(engines),
    })
}

fn set_format(slot: &mut Option<Format>, format: Format) -> Result<(), String> {
    if slot.replace(format).is_some() {
        return Err("--json, --table and --urls are mutually exclusive".to_string());
    }
    Ok(())
}
