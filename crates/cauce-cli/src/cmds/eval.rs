//! `cauce eval engines <cases.jsonl>... [--live]`: engine relevance evals
//! (W3-05). Each case line names the engines to run its query against;
//! scoring is domain-hit@5 — a case hits when an expected domain shows up
//! in the host of one of the engine's first five normalized result URLs.
//! The run writes `evals/results/<date>-engines.json` (or `--results-dir`),
//! which `/api/stats` then exposes and the nightly workflow uploads.
//!
//! Without `--live` the named engines are replay engines pinned to their
//! cassette directory under `--fixtures-dir` (`engines/fixtures`); a query
//! with no cassette is a miss, never a synthetic result. With `--live` the
//! engines come from the same config/auto-registration `cauce serve` uses,
//! so the per-engine politeness bucket applies — the nightly runs this.
//!
//! The command always exits 0 once the eval ran: below-threshold scores are
//! a report outcome (`below_threshold` in the JSON), not a CLI failure —
//! the workflow turns them into one tracking issue and never gates merges.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cauce_core::config::Config;
use cauce_core::evals::{
    CaseOutcome, EngineScore, EvalCase, EvalReport, Thresholds, domain_hit, load_cases, top_hosts,
    write_report,
};
use cauce_core::{ClientKind, Engine, EngineId, SafeSearch, SearchRequest};
use cauce_engines::cassette::cassette_path;
use cauce_engines::factory::{build_engine, build_engines};
use cauce_engines::{Replay, ReplayOpts};

const USAGE: &str = "usage: cauce eval engines <cases.jsonl>... [--live] [--fixtures-dir <engines/fixtures>] [--results-dir <evals/results>] [--thresholds <evals/thresholds.toml>]";

/// Budget for one engine call — live fetches bound their own
/// `request.timeout_ms` inside it, replay answers instantly.
const CASE_BUDGET: Duration = Duration::from_secs(30);
/// How many top results the domain check sees (@5 is the scored metric).
const TOP_N: usize = 5;

struct EvalArgs {
    files: Vec<PathBuf>,
    live: bool,
    fixtures_dir: PathBuf,
    results_dir: PathBuf,
    thresholds: PathBuf,
}

/// Entry point for the `eval` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(msg) => {
            eprintln!("cauce eval: {msg}\n{USAGE}");
            return 2;
        }
    };
    match run_inner(&opts) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("cauce eval: {msg}");
            1
        }
    }
}

fn run_inner(opts: &EvalArgs) -> Result<i32, String> {
    let mut cases = Vec::new();
    for path in &opts.files {
        cases.extend(load_cases(path).map_err(|e| e.to_string())?);
    }
    if cases.is_empty() {
        return Err("no eval cases in the given files".to_string());
    }
    let thresholds = Thresholds::load(&opts.thresholds).map_err(|e| e.to_string())?;

    let engines = build_eval_engines(opts, &cases)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let report = rt.block_on(score_cases(&cases, &engines, &thresholds, opts));
    let path = write_report(&opts.results_dir, &report).map_err(|e| e.to_string())?;
    eprintln!("wrote {}", path.display());

    print_summary(&report);
    Ok(0)
}

/// The engine set: one instance per distinct id named by the cases. Live
/// resolves through config entries plus spec auto-registration (the same
/// set `build_engines` produces, minus engines no case named); replay
/// builds a cassette-pinned [`Replay`] per id.
fn build_eval_engines(
    opts: &EvalArgs,
    cases: &[EvalCase],
) -> Result<BTreeMap<EngineId, Arc<dyn Engine>>, String> {
    let mut named: Vec<EngineId> = cases
        .iter()
        .flat_map(|c| c.engines.iter().cloned())
        .collect();
    named.sort();
    named.dedup();

    let mut map = BTreeMap::new();
    if opts.live {
        let cfg = Config::load().map_err(|e| format!("config load: {e}"))?;
        let available: BTreeMap<EngineId, Arc<dyn Engine>> = build_engines(&cfg)
            .into_iter()
            .map(|e| (e.id(), e))
            .collect();
        for id in &named {
            let engine = available.get(id).cloned().or_else(|| {
                cfg.engine(id.as_str())
                    .and_then(|entry| build_engine(entry, cfg.config_dir()))
            });
            let Some(engine) = engine else {
                let known = available
                    .keys()
                    .map(EngineId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!("unknown engine {id:?} (available: {known})"));
            };
            map.insert(id.clone(), engine);
        }
    } else {
        for id in named {
            let replay = ReplayOpts {
                id: id.clone(),
                cassette_engine: Some(id.clone()),
                fixtures_root: opts.fixtures_dir.clone(),
                ..Default::default()
            };
            map.insert(id, Arc::new(Replay::new(replay)) as Arc<dyn Engine>);
        }
    }
    Ok(map)
}

/// Run every (case, engine) pair serially — politeness is per-engine, so
/// fan-out parallelism would buy nothing a token bucket doesn't already
/// smooth.
async fn score_cases(
    cases: &[EvalCase],
    engines: &BTreeMap<EngineId, Arc<dyn Engine>>,
    thresholds: &Thresholds,
    opts: &EvalArgs,
) -> EvalReport {
    let mut outcomes = Vec::new();
    for case in cases {
        for engine_id in &case.engines {
            let engine = engines[engine_id].clone();
            outcomes.push(score_one(case, engine.as_ref(), opts).await);
        }
    }
    aggregate(cases, outcomes, thresholds, opts.live)
}

async fn score_one(case: &EvalCase, engine: &dyn Engine, opts: &EvalArgs) -> CaseOutcome {
    // Replay mode serves cassettes only: a missing cassette is a miss with
    // a note, never a seeded synthetic page that could fake a hit.
    if !opts.live && !cassette_path(&opts.fixtures_dir, engine.id().as_str(), &case.query).is_file()
    {
        return CaseOutcome {
            engine: engine.id(),
            query: case.query.clone(),
            expect_domains_top5: case.expect_domains_top5.clone(),
            hit: false,
            top5_hosts: vec![],
            note: Some(format!(
                "no cassette {}",
                cassette_path(&opts.fixtures_dir, engine.id().as_str(), &case.query).display()
            )),
        };
    }
    let req = SearchRequest {
        q: case.query.clone(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: Some(vec![engine.id()]),
        client: ClientKind::Cli,
    };
    match engine.search(&req, CASE_BUDGET).await {
        Ok(results) => {
            let hosts = top_hosts(&results, TOP_N);
            CaseOutcome {
                engine: engine.id(),
                query: case.query.clone(),
                expect_domains_top5: case.expect_domains_top5.clone(),
                hit: domain_hit(&hosts, &case.expect_domains_top5),
                top5_hosts: hosts,
                note: None,
            }
        }
        Err(e) => CaseOutcome {
            engine: engine.id(),
            query: case.query.clone(),
            expect_domains_top5: case.expect_domains_top5.clone(),
            hit: false,
            top5_hosts: vec![],
            note: Some(e.to_string()),
        },
    }
}

fn aggregate(
    cases: &[EvalCase],
    outcomes: Vec<CaseOutcome>,
    thresholds: &Thresholds,
    live: bool,
) -> EvalReport {
    let mut per_engine: BTreeMap<EngineId, (u32, u32)> = BTreeMap::new();
    for case in cases {
        for id in &case.engines {
            per_engine.entry(id.clone()).or_default().0 += 1;
        }
    }
    for o in &outcomes {
        if o.hit {
            per_engine.entry(o.engine.clone()).or_default().1 += 1;
        }
    }
    let mut engines = Vec::new();
    let mut below = Vec::new();
    for (engine, (cases_n, hits)) in per_engine {
        let score = if cases_n == 0 {
            0.0
        } else {
            hits as f64 / cases_n as f64
        };
        let threshold = thresholds.for_engine(engine.as_str());
        let ok = score >= threshold;
        if !ok {
            below.push(engine.clone());
        }
        engines.push(EngineScore {
            engine,
            cases: cases_n,
            hits,
            domain_hit_at5: score,
            threshold,
            ok,
        });
    }
    EvalReport::new(live, engines, below, outcomes)
}

fn print_summary(report: &EvalReport) {
    for row in &report.engines {
        println!(
            "{:<12} {}/{} domain-hit@5 = {:.2} (threshold {:.2}) {}",
            row.engine,
            row.hits,
            row.cases,
            row.domain_hit_at5,
            row.threshold,
            if row.ok { "ok" } else { "BELOW" }
        );
    }
    if !report.below_threshold.is_empty() {
        eprintln!(
            "below threshold: {}",
            report
                .below_threshold
                .iter()
                .map(EngineId::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// `Ok(None)` is `-h`/`--help`: usage goes to stdout with exit 0, not
/// through the error path.
fn parse(args: &[String]) -> Result<Option<EvalArgs>, String> {
    if matches!(
        args.first().map(String::as_str),
        Some("-h") | Some("--help")
    ) {
        return Ok(None);
    }
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("engines") {
        return Err("expected `cauce eval engines <cases.jsonl>...`".to_string());
    }
    let mut files = Vec::new();
    let mut live = false;
    let mut fixtures_dir = PathBuf::from("engines/fixtures");
    let mut results_dir = cauce_core::evals::results_dir();
    let mut thresholds = PathBuf::from("evals/thresholds.toml");
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
            "--live" => live = true,
            "--fixtures-dir" => fixtures_dir = PathBuf::from(value("--fixtures-dir")?),
            "--results-dir" => results_dir = PathBuf::from(value("--results-dir")?),
            "--thresholds" => thresholds = PathBuf::from(value("--thresholds")?),
            "-h" | "--help" => return Ok(None),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag {other:?}"));
            }
            positional => files.push(PathBuf::from(positional)),
        }
    }
    if files.is_empty() {
        return Err("missing <cases.jsonl>".to_string());
    }
    Ok(Some(EvalArgs {
        files,
        live,
        fixtures_dir,
        results_dir,
        thresholds,
    }))
}
