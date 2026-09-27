//! `cauce eval ai <cases.jsonl>... [--tag <tag>]... [--gate] [--record]`:
//! grounded-answer evals (W4-04). Each case replays a recorded provider
//! transcript (`evals/ai/transcripts/<stem>.json`) through
//! `AnswerLoop::stream_answer` with the shared `search_web` tool backed
//! by a replay engine pinned to `evals/ai/cassettes` — no provider calls,
//! no live engines.
//!
//! Cases run in parallel on a multi-thread runtime sized from available
//! cores; the report's `outcomes` stay in case-file order (deterministic,
//! not completion order). Scoring per case is the mean of cited-domain
//! recall, `must_contain` hits and `must_not_contain` cleanliness; the
//! run writes `evals/results/<date>-ai.json`.
//!
//! `--gate` turns the run into the CI fast gate: exit 1 when the score
//! drops below `[ai].baseline - tolerance` from `evals/thresholds.toml`
//! (or `ungrounded_rate` tops `ungrounded_max`). Without it the command
//! always exits 0 once the eval ran — scores are a report outcome, not a
//! CLI failure, same as `eval engines`.
//!
//! `--record` (owner-run) rebuilds every case's transcript against the
//! live `[ai]` provider instead of replaying it, overwriting
//! `transcripts/<stem>.json` — then scores the fresh run like any other.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::sync::Arc;

use cauce_core::ai::{AnswerLoop, AnswerRequest, ChatProvider, provider_client};
use cauce_core::config::{AiProtocol, Config};
use cauce_core::evals::ai::{
    AiCaseOutcome, AiEvalCase, AiEvalReport, AiThresholds, RecordingProvider, TranscriptProvider,
    load_cases, load_transcript, record_stem, save_transcript, score_frames, tagged,
    transcript_variants, write_report,
};
use cauce_core::evals::{Thresholds, results_dir};
use cauce_core::{ClientKind, Engine, EngineId, SearchPipeline, Store, StoreTuning};
use cauce_engines::cassette::cassette_path;
use cauce_engines::{Replay, ReplayOpts};
use cauce_store_sqlite::SqliteStore;
use tokio::task::JoinSet;

const USAGE: &str = "usage: cauce eval ai <cases.jsonl>... [--tag <tag>]... [--gate] [--record] [--fixtures-dir <evals/ai/cassettes>] [--transcripts-dir <evals/ai/transcripts>] [--results-dir <evals/results>] [--thresholds <evals/thresholds.toml>]";

/// Engine id the replay serves under — its cassettes live at
/// `<fixtures-dir>/replay/<sha8>.json`.
const REPLAY_ENGINE: &str = "replay";

#[derive(Clone)]
struct EvalAiArgs {
    files: Vec<PathBuf>,
    /// Tag filter; empty runs every case.
    tags: Vec<String>,
    /// Exit 1 when the score is below `baseline - tolerance`.
    gate: bool,
    /// Rebuild transcripts against the live `[ai]` provider (owner-run).
    record: bool,
    /// Replay cassette root (`evals/ai/cassettes`).
    fixtures_dir: PathBuf,
    /// Transcript directory (`evals/ai/transcripts`).
    transcripts_dir: PathBuf,
    results_dir: PathBuf,
    thresholds: PathBuf,
}

/// Entry point for `cauce eval ai`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(msg) => {
            eprintln!("cauce eval ai: {msg}\n{USAGE}");
            return 2;
        }
    };
    match run_inner(opts) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("cauce eval ai: {msg}");
            1
        }
    }
}

fn run_inner(opts: EvalAiArgs) -> Result<i32, String> {
    let mut cases = Vec::new();
    for path in &opts.files {
        cases.extend(load_cases(path).map_err(|e| e.to_string())?);
    }
    let cases = tagged(cases, &opts.tags);
    if cases.is_empty() {
        return Err("no eval cases matched".to_string());
    }
    let thresholds = Thresholds::load(&opts.thresholds).map_err(|e| e.to_string())?;
    let ai: AiThresholds = thresholds.ai.ok_or_else(|| {
        format!(
            "{}: no [ai] section (baseline/tolerance) — the gate has nothing to compare against",
            opts.thresholds.display()
        )
    })?;

    // A live provider exists only under --record; transcript replays
    // need none. `[ai].protocol` picks the client (W4-05); an
    // anthropic recording writes `<stem>.anthropic.json`.
    let record_client: Option<(Arc<dyn ChatProvider>, String, AiProtocol)> = if opts.record {
        let cfg = Config::load().map_err(|e| format!("config load: {e}"))?;
        if !cfg.ai.enabled {
            eprintln!(
                "cauce eval ai: [ai].enabled is false; recording anyway (--record is explicit)"
            );
        }
        let client =
            provider_client(&cfg.ai, None).map_err(|e| format!("[ai] provider config: {e}"))?;
        if client.model().is_empty() {
            return Err("[ai].model is empty — set it before recording transcripts".to_string());
        }
        Some((client, cfg.ai.base_url.clone(), cfg.ai.protocol))
    } else {
        None
    };

    let replay: Arc<dyn Engine> = Arc::new(Replay::new(ReplayOpts {
        id: EngineId::from(REPLAY_ENGINE),
        cassette_engine: Some(EngineId::from(REPLAY_ENGINE)),
        fixtures_root: opts.fixtures_dir.clone(),
        ..Default::default()
    }));

    let workers = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    let opts = Arc::new(opts);
    let outcomes = rt.block_on(run_cases(&cases, opts.clone(), replay, record_client));
    let report = AiEvalReport::new(&ai, opts.tags.clone(), outcomes);
    let path = write_report(&opts.results_dir, &report).map_err(|e| e.to_string())?;
    eprintln!("wrote {}", path.display());

    print_summary(&report, &ai);
    if opts.gate && !report.gate_ok {
        eprintln!(
            "cauce eval ai: gate failed — score {:.2} below baseline {:.2} - tolerance {:.2}",
            report.score, ai.baseline, ai.tolerance
        );
        return Ok(1);
    }
    Ok(0)
}

/// Spawn one task per case, collect through a `JoinSet` and restore
/// case-file order before reporting — parallelism changes completion
/// order, never output order.
async fn run_cases(
    cases: &[AiEvalCase],
    opts: Arc<EvalAiArgs>,
    replay: Arc<dyn Engine>,
    record_client: Option<(Arc<dyn ChatProvider>, String, AiProtocol)>,
) -> Vec<AiCaseOutcome> {
    // One shared temp dir; each run gets a private `case-<i>-<v>`
    // subdir so `answers` cache writes never leak across runs.
    let work_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            return cases
                .iter()
                .map(|case| AiCaseOutcome::skipped(case, format!("temp dir: {e}")))
                .collect();
        }
    };

    // One run per case per transcript variant on disk (W4-05):
    // `<stem>.json` plus each `<stem>.<proto>.json`. Under --record the
    // configured protocol runs once per case and writes its variant.
    let mut units: Vec<(usize, usize, AiEvalCase, String, String)> = Vec::new();
    for (ci, case) in cases.iter().enumerate() {
        match &record_client {
            Some((_, _, proto)) => units.push((
                ci,
                0,
                case.clone(),
                record_stem(&case.transcript, proto.as_str()),
                proto.as_str().to_string(),
            )),
            None => {
                for (vi, (stem, proto)) in
                    transcript_variants(&opts.transcripts_dir, &case.transcript)
                        .into_iter()
                        .enumerate()
                {
                    units.push((ci, vi, case.clone(), stem, proto));
                }
            }
        }
    }

    let mut set = JoinSet::new();
    for (ci, vi, case, stem, proto) in units {
        let opts = Arc::clone(&opts);
        let replay = Arc::clone(&replay);
        let record_client = record_client.clone();
        let db_dir = work_dir.path().join(format!("case-{ci}-{vi}"));
        set.spawn(async move {
            (
                (ci, vi),
                run_case(case, stem, proto, opts, replay, record_client, db_dir).await,
            )
        });
    }

    let mut outcomes: Vec<((usize, usize), AiCaseOutcome)> = Vec::with_capacity(set.len());
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(pair) => outcomes.push(pair),
            // A panicked case is still a case — score it 0 with the panic
            // as the note rather than dropping it (a short report would
            // inflate the mean).
            Err(e) => outcomes.push(((usize::MAX, usize::MAX), panic_outcome(&e))),
        }
    }
    outcomes.sort_by_key(|(idx, _)| *idx);
    outcomes.into_iter().map(|(_, o)| o).collect()
}

fn panic_outcome(e: &tokio::task::JoinError) -> AiCaseOutcome {
    AiCaseOutcome {
        query: String::new(),
        transcript: String::new(),
        score: 0.0,
        ok: false,
        cited_recall: 0.0,
        contain_rate: 0.0,
        clean_rate: 0.0,
        cited_hosts: vec![],
        missing_domains: vec![],
        missing_strings: vec![],
        leaked_strings: vec![],
        ungrounded: true,
        note: Some(format!("case task panicked: {e}")),
        protocol: "openai".to_string(),
    }
}

/// Run one case end to end on one transcript variant: transcript
/// `<stem>.json` (or the live provider under `--record`) →
/// `stream_answer` → scored frames. `stem`/`proto` name the variant —
/// `("tokyo-weather", "openai")` or `("tokyo-weather.anthropic",
/// "anthropic")` — and are stamped on the outcome.
async fn run_case(
    case: AiEvalCase,
    stem: String,
    proto: String,
    opts: Arc<EvalAiArgs>,
    replay: Arc<dyn Engine>,
    record_client: Option<(Arc<dyn ChatProvider>, String, AiProtocol)>,
    db_dir: PathBuf,
) -> AiCaseOutcome {
    let stamp = |mut o: AiCaseOutcome| {
        o.transcript = stem.clone();
        o.protocol = proto.clone();
        o
    };
    if let Err(e) = std::fs::create_dir_all(&db_dir) {
        return stamp(AiCaseOutcome::skipped(&case, format!("temp dir: {e}")));
    }
    let store: Arc<dyn Store> =
        match SqliteStore::open(db_dir.join("eval.db"), StoreTuning::default()) {
            Ok(s) => Arc::new(s),
            Err(e) => return stamp(AiCaseOutcome::skipped(&case, format!("store open: {e}"))),
        };

    // Record wraps the live client; replay loads the transcript. A case
    // whose transcript is missing/corrupt scores 0 with the load error as
    // its note — never a silent skip.
    let (provider, recorder): (Arc<dyn ChatProvider>, Option<Arc<RecordingProvider>>) =
        match &record_client {
            Some((client, base_url, proto)) => {
                let rec = Arc::new(
                    RecordingProvider::new(client.clone())
                        .with_provider_label(base_url.clone())
                        .with_protocol(proto.as_str()),
                );
                (rec.clone() as Arc<dyn ChatProvider>, Some(rec))
            }
            None => match load_transcript(&opts.transcripts_dir, &stem) {
                Ok(t) => (Arc::new(TranscriptProvider::new(t)), None),
                Err(e) => {
                    return stamp(AiCaseOutcome::skipped(&case, format!("transcript: {e}")));
                }
            },
        };

    let pipeline = SearchPipeline::new(store.clone(), vec![replay]);
    let loop_ = AnswerLoop::new(pipeline, provider, store);
    let req = AnswerRequest {
        q: case.query.clone(),
        history: Vec::new(),
        client: ClientKind::Cli,
        request_id: None,
        actor: Some("eval-ai".to_string()),
    };
    let mut rx = loop_.stream_answer(&req);
    let mut frames = Vec::new();
    while let Some(frame) = rx.recv().await {
        frames.push(frame);
    }

    let mut outcome = stamp(score_frames(&case, &frames));

    // A tool query with no cassette silently falls back to synthetic
    // replay results — flag those on the outcome so a case that only
    // passes on fabricated hosts is visible in the report.
    let synthetic: Vec<String> = frames
        .iter()
        .filter_map(|f| match f {
            cauce_core::ai::AnswerFrame::Step { query, .. } => Some(query.clone()),
            _ => None,
        })
        .filter(|q| !cassette_path(&opts.fixtures_dir, REPLAY_ENGINE, q).is_file())
        .collect();
    if !synthetic.is_empty() {
        let note = format!(
            "no cassette (synthetic replay results) for: {}",
            synthetic.join(", ")
        );
        outcome.note = Some(match outcome.note.take() {
            Some(prev) => format!("{prev}; {note}"),
            None => note,
        });
    }

    if let Some(rec) = recorder
        && let Err(e) = save_transcript(&opts.transcripts_dir, &stem, &rec.transcript())
    {
        outcome.note = Some(match outcome.note.take() {
            Some(prev) => format!("{prev}; transcript save: {e}"),
            None => format!("transcript save: {e}"),
        });
    }
    outcome
}

fn print_summary(report: &AiEvalReport, thresholds: &AiThresholds) {
    for o in &report.outcomes {
        // Non-default protocol variants are marked on the line.
        let variant = match o.protocol.as_str() {
            "openai" => String::new(),
            proto => format!(" [{proto}]"),
        };
        println!(
            "{:<40} score {:.2} cited {:.2} contain {:.2} clean {:.2} {}{}{}",
            truncate(&o.query, 40),
            o.score,
            o.cited_recall,
            o.contain_rate,
            o.clean_rate,
            if o.ok { "ok" } else { "FAIL" },
            variant,
            o.note
                .as_deref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default(),
        );
    }
    println!(
        "ai evals: score {:.2} over {} cases ({}/{} ok, baseline {:.2} tolerance {:.2}) — ungrounded {}/{}",
        report.score,
        report.cases,
        report.ok_cases,
        report.cases,
        thresholds.baseline,
        thresholds.tolerance,
        report.ungrounded_cases.len(),
        report.cases,
    );
}

/// Display-trim a query to `n` chars for the summary table.
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut t: String = s.chars().take(n - 1).collect();
    t.push('…');
    t
}

/// `Ok(None)` is `-h`/`--help`: usage goes to stdout with exit 0, not
/// through the error path.
fn parse(args: &[String]) -> Result<Option<EvalAiArgs>, String> {
    let mut it = args.iter();
    let mut files = Vec::new();
    let mut tags = Vec::new();
    let mut gate = false;
    let mut record = false;
    let mut fixtures_dir = PathBuf::from("evals/ai/cassettes");
    let mut transcripts_dir = PathBuf::from("evals/ai/transcripts");
    let mut results_dir = results_dir();
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
            "--tag" => tags.push(value("--tag")?),
            "--gate" => gate = true,
            "--record" => record = true,
            "--fixtures-dir" => fixtures_dir = PathBuf::from(value("--fixtures-dir")?),
            "--transcripts-dir" => transcripts_dir = PathBuf::from(value("--transcripts-dir")?),
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
    Ok(Some(EvalAiArgs {
        files,
        tags,
        gate,
        record,
        fixtures_dir,
        transcripts_dir,
        results_dir,
        thresholds,
    }))
}
