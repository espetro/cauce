//! `oxe record --engine <id> --query <q>`: run an engine once and write a
//! replay cassette under `engines/fixtures/<id>/<sha8>.json`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;

use oxe_core::Engine;
use oxe_engines::exec::{ExecEngine, ExecSpec};
use oxe_engines::{Replay, record};

const USAGE: &str = "usage: oxe record --engine <id> --query <q> [--out-dir <engines/fixtures>]";

/// Entry point for the `record` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
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

/// Map `--engine <id>` to a constructor (W0-06: only the engines that exist
/// in wave 0).
fn engine_for(id: &str) -> Result<Box<dyn Engine>, String> {
    match id {
        "replay" => Ok(Box::new(Replay::from_env())),
        "ddgs" => Ok(Box::new(ExecEngine::new(ExecSpec::ddgs(repo_root())))),
        other => Err(format!(
            "unknown engine {other:?} (supported: ddgs, replay)"
        )),
    }
}

/// Directory the relative `sdk/python/oxe_engine_sdk/ddgs_auto.py` arg of
/// `ExecSpec::ddgs` resolves against. `oxe record` is a repo-local dev tool:
/// walk up from the process cwd until the script is found (works from any
/// subdirectory of a checkout), else fall back to the compile-time
/// `CARGO_MANIFEST_DIR` root, which covers `cargo run` from anywhere.
fn repo_root() -> PathBuf {
    const SCRIPT: &str = "sdk/python/oxe_engine_sdk/ddgs_auto.py";
    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            if dir.join(SCRIPT).is_file() {
                return dir;
            }
            if !dir.pop() {
                break;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_inner(opts: RecordArgs) -> Result<PathBuf, String> {
    let engine = engine_for(&opts.engine)?;
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
