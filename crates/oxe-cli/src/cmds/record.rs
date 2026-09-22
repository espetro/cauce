//! `oxe record --engine <id> --query <q>`: run an engine once and write a
//! replay cassette under `engines/fixtures/<id>/<sha8>.json`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::sync::Arc;

use oxe_core::Engine;
use oxe_core::config::Config;
use oxe_engines::factory::build_engine;
use oxe_engines::record;

const USAGE: &str = "usage: oxe record --engine <id> --query <q> [--out-dir <engines/fixtures>]";

/// Entry point for the `record` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    // Exec children forward their stderr through `tracing` at warn; install
    // a minimal stderr-only subscriber so those lines are visible. The
    // JSONL/OTLP observability pipeline is `serve`'s, not the recorder's —
    // and stdout stays clean for the printed cassette path.
    let filter = std::env::var("OXE_LOG")
        .ok()
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| "warn".to_string());
    let filter = tracing_subscriber::EnvFilter::try_new(filter)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    match parse(args).and_then(run_inner) {
        Ok(path) => {
            println!("{}", path.display());
            0
        }
        Err(msg) => {
            eprintln!("oxe record: {msg}\n{USAGE}");
            2
        }
    }
}

/// Resolve `--engine <id>` through the same factory `serve` uses:
/// `Config::load()` picks up `[[engines]]` entries and `OXE_ENGINES`, and
/// `build_engine` applies the entry's `command`/`args`/`env`/`cwd`/`tier`/
/// `page_size`. Disabled entries are recordable — `engine()` looks up by
/// id, not by enabled.
fn engine_for(cfg: &Config, id: &str) -> Result<Arc<dyn Engine>, String> {
    let entry = cfg.engine(id).ok_or_else(|| {
        let known = cfg
            .engines
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown engine {id:?} (known: {known})")
    })?;
    build_engine(entry).ok_or_else(|| {
        format!(
            "engine {id:?} is not runnable in wave 0 (kind {:?})",
            entry.kind
        )
    })
}

fn run_inner(opts: RecordArgs) -> Result<PathBuf, String> {
    let cfg = Config::load().map_err(|e| format!("config load: {e}"))?;
    let engine = engine_for(&cfg, &opts.engine)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(record(engine.as_ref(), &opts.query, &opts.out_dir))
        .map_err(|e| e.to_string())
}

struct RecordArgs {
    engine: String,
    query: String,
    out_dir: PathBuf,
}

fn parse(args: &[String]) -> Result<RecordArgs, String> {
    let mut engine = None;
    let mut query = None;
    let mut out_dir = PathBuf::from("engines/fixtures");

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
            "--engine" | "-e" => engine = Some(value("--engine")?),
            "--query" | "-q" => query = Some(value("--query")?),
            "--out-dir" | "-o" => out_dir = PathBuf::from(value("--out-dir")?),
            "-h" | "--help" => return Err("help requested".into()),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }

    Ok(RecordArgs {
        engine: engine.ok_or("missing --engine <id>")?,
        query: query.ok_or("missing --query <q>")?,
        out_dir,
    })
}
