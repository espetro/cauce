//! `cauce report [--out PATH] [--days N] [--include-queries] [--print]
//! [--gh]` (#242): assemble the support-report bundle in-process — the
//! same bundle `GET /api/report` serves — write it, and print the
//! prefilled `issues/new` URL. `--gh` hands the same draft to `gh issue
//! create` when `gh` is on PATH.
//!
//! Wiring mirrors `cmds::search`: `Config::load` -> `SqliteStore` ->
//! engine factory -> `SearchPipeline` -> `AppState` -> the
//! `cauce_server::report` collector. The command needs no server and
//! initializes no observability — it reads the JSONL logs, it does not
//! append to them.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use cauce_core::SearchPipeline;
use cauce_core::config::{Config, Resources};
use cauce_core::report::{self, ReportBundle};
use cauce_engines::factory::build_engines;
use cauce_server::AppState;
use cauce_store_sqlite::SqliteStore;

const USAGE: &str = "usage: cauce report [--out <path>] [--days <n>] \
                     [--include-queries] [--print] [--gh]";

/// The stats/audit/log window `report` collects when `--days` is absent —
/// the `/api/report` default and the design's `stats(7d)`.
const DEFAULT_DAYS: u32 = 7;

struct ReportOpts {
    /// `--out <path>`; unset writes `cauce-report-<ts>.json` under cwd.
    out: Option<PathBuf>,
    days: u32,
    /// `--include-queries`: the verbose profile for this export only.
    include_queries: bool,
    /// `--print`: emit the bundle JSON on stdout instead of writing a
    /// file (with `--out` it does both).
    print: bool,
    /// `--gh`: file the issue via `gh issue create` when `gh` is on PATH.
    gh: bool,
}

/// Entry point for the `report` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(msg) => {
            eprintln!("cauce report: {msg}\n{USAGE}");
            return 2;
        }
    };
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("cauce report: {e}");
            return 2;
        }
    };
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("cauce report: cannot create data dirs: {e}");
        return 1;
    }
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cauce report: tokio runtime: {e}");
            return 1;
        }
    };
    rt.block_on(report_async(&cfg, &opts))
}

async fn report_async(cfg: &Config, opts: &ReportOpts) -> i32 {
    let tuning = Resources::detect().store_tuning;
    let store = match SqliteStore::open(cfg.db_path(), tuning) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            eprintln!(
                "cauce report: cannot open store {}: {e}",
                cfg.db_path().display()
            );
            return 1;
        }
    };
    let engines = build_engines(cfg);
    let pipeline = Arc::new(SearchPipeline::from_config(cfg, store.clone(), engines));
    // Persisted breakers are half the story in an incident report;
    // a load failure degrades to all-Closed like `serve` startup.
    if let Err(e) = pipeline.load_health().await {
        eprintln!("cauce report: engine health load failed ({e}); reporting closed breakers");
    }
    let state = AppState::new(pipeline, store, cfg.clone());
    let bundle = cauce_server::report::collect(&state, opts.days, opts.include_queries).await;
    let json = bundle.to_json();

    // `--print` puts the bundle on stdout, so the status lines slide to
    // stderr to keep `cauce report --print | jq` clean.
    let status = |msg: String| {
        if opts.print {
            eprintln!("{msg}");
        } else {
            println!("{msg}");
        }
    };
    let mut wrote = None;
    if !opts.print || opts.out.is_some() {
        let path = opts
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from(report::filename(&bundle.generated_at)));
        if let Err(e) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("cauce report: cannot write {}: {e}", path.display());
            return 1;
        }
        status(format!("wrote {}", path.display()));
        wrote = Some(path);
    }
    if opts.print {
        println!("{json}");
    }
    status(format!("issue: {}", report::issue_url(&bundle)));
    if opts.gh {
        return gh_issue_create(&bundle, wrote.as_deref());
    }
    0
}

/// `gh issue create` for the bundle's [`report::issue_draft`]; the body
/// names the written file so the issue text still points at it. Missing
/// `gh` is a 1 with the URL printed instead.
fn gh_issue_create(bundle: &ReportBundle, file: Option<&Path>) -> i32 {
    let draft = report::issue_draft(bundle);
    let body = match file {
        Some(path) => format!("{}\n\nBundle file: {}", draft.body, path.display()),
        None => draft.body,
    };
    match Command::new("gh")
        .args([
            "issue",
            "create",
            "--repo",
            "espetro/cauce",
            "--title",
            &draft.title,
            "--body",
            &body,
        ])
        .status()
    {
        Ok(status) if status.success() => 0,
        Ok(status) => {
            eprintln!("cauce report: `gh issue create` exited {status}");
            status.code().unwrap_or(1)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "cauce report: `gh` is not on PATH; open the issue manually:\n{}",
                report::issue_url(bundle)
            );
            1
        }
        Err(e) => {
            eprintln!("cauce report: cannot run `gh`: {e}");
            1
        }
    }
}

/// `Ok(None)` is `-h`/`--help`: usage goes to stdout with exit 0, not
/// through the error path.
fn parse(args: &[String]) -> Result<Option<ReportOpts>, String> {
    let mut opts = ReportOpts {
        out: None,
        days: DEFAULT_DAYS,
        include_queries: false,
        print: false,
        gh: false,
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
            "--out" | "-o" => opts.out = Some(PathBuf::from(value(flag)?)),
            "--days" => {
                let raw = value(flag)?;
                opts.days = raw
                    .parse::<u32>()
                    .map_err(|_| format!("invalid --days {raw:?}"))?;
                if opts.days == 0 {
                    return Err("--days must be at least 1".into());
                }
            }
            "--include-queries" => opts.include_queries = true,
            "--print" => opts.print = true,
            "--gh" => opts.gh = true,
            "-h" | "--help" => return Ok(None),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    Ok(Some(opts))
}
