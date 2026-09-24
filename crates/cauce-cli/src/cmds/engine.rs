//! `cauce engine test <spec.yaml>`: run the declarative spec's fixture pairs
//! offline (`engines/fixtures/<id>/<name>.html|.json` +
//! `<name>.expected.json`), `--live "<query>"` to fetch once and print the
//! parsed results, `--record` (with `--live "<query>"`) to write a new
//! fixture pair, and a bare `--live` (W3-06) to run the nightly drift
//! canary — page 1 + page 2 of the committed fixture's recorded query —
//! over `<spec.yaml>` or every embedded spec.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::slice::Iter;
use std::time::Duration;

use cauce_core::config::{Config, system_env};
use cauce_core::http::HttpClient;
use cauce_core::{ClientKind, Engine, EngineId, SafeSearch, SearchRequest};
use cauce_engines::declarative::canary::{self, CanaryReport};
use cauce_engines::declarative::fixtures::{
    FixtureError, FixtureReport, compile_spec_source, fixture_pairs, run_pair, write_pair,
};
use cauce_engines::declarative::loading::{SpecSource, load_specs_report};
use cauce_engines::declarative::{CompiledSpec, DeclarativeEngine};

const USAGE: &str = "usage: cauce engine test [<spec.yaml>] [--live [\"<query>\"]] [--record] \
                     [--fixtures-dir <engines/fixtures>]\n  bare `--live` runs the drift canary \
                     over <spec.yaml>, or over every embedded spec when no spec is given";

/// Budget for one `--live` fetch (capped further by the spec's
/// `request.timeout_ms`).
const LIVE_BUDGET: Duration = Duration::from_secs(30);

/// Entry point for the `engine` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let result = match parse(args) {
        Ok(Some(opts)) => dispatch(opts),
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(msg) => Err(msg),
    };
    match result {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("cauce engine: {msg}\n{USAGE}");
            2
        }
    }
}

fn dispatch(opts: EngineArgs) -> Result<i32, String> {
    let config_dir = cauce_core::config::Dirs::detect().config_dir;
    let env = system_env();
    let spec = match &opts.spec {
        Some(path) => {
            Some(compile_spec_source(path, &config_dir, &env).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    match &opts.live {
        Some(Live::Canary) => run_canary(spec, &opts, &config_dir, &env),
        Some(Live::Query(query)) => {
            let spec = spec.ok_or_else(|| "missing <spec.yaml>".to_string())?;
            run_live(spec, query, &opts)
        }
        None => {
            let spec = spec.ok_or_else(|| "missing <spec.yaml>".to_string())?;
            run_fixtures(&spec, &opts)
        }
    }
}

/// Offline mode: every fixture pair under `<fixtures_dir>/<id>/`.
fn run_fixtures(spec: &CompiledSpec, opts: &EngineArgs) -> Result<i32, String> {
    let pairs = fixture_pairs(&opts.fixtures_dir, spec.id().as_str()).map_err(|e| e.to_string())?;
    if pairs.is_empty() {
        return Err(format!(
            "no fixture pairs under {}",
            opts.fixtures_dir.join(spec.id().as_str()).display()
        ));
    }
    let mut failures = 0;
    for pair in &pairs {
        match run_pair(spec, pair) {
            Ok(FixtureReport {
                name,
                outcome: Ok(n),
            }) => println!("PASS {name} ({n} results)"),
            Ok(FixtureReport {
                name,
                outcome: Err(reason),
            }) => {
                failures += 1;
                println!("FAIL {name}: {reason}");
            }
            Err(e) => {
                failures += 1;
                println!("FAIL {}: {e}", pair.name);
            }
        }
    }
    println!(
        "{} fixture(s), {} passed, {} failed",
        pairs.len(),
        pairs.len() - failures,
        failures
    );
    Ok(if failures == 0 { 0 } else { 1 })
}

/// `--live "<query>"`: one real fetch through the engine's configured
/// egress; `--record` additionally writes the fixture pair.
fn run_live(spec: CompiledSpec, query: &str, opts: &EngineArgs) -> Result<i32, String> {
    let (engine, _rt) = live_engine(spec)?;
    let req = SearchRequest {
        q: query.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: Some(vec![engine.id()]),
        client: ClientKind::Cli,
    };
    let fetched = _rt
        .block_on(engine.fetch(&req, LIVE_BUDGET))
        .map_err(|e| format!("fetch: {e}"))?;
    let parsed = engine.parse_response(fetched.status, &fetched.body, &fetched.url);

    match &parsed {
        Ok(results) => {
            println!(
                "{}",
                serde_json::to_string_pretty(results).map_err(|e| e.to_string())?
            );
        }
        Err(e) => eprintln!("parse: {e}"),
    }
    if opts.record {
        let path = write_pair(
            &opts.fixtures_dir,
            engine.compiled(),
            query,
            fetched.status,
            &fetched.body,
            &parsed,
        )
        .map_err(|e: FixtureError| e.to_string())?;
        eprintln!("wrote {}", path.display());
    }
    Ok(if parsed.is_ok() { 0 } else { 1 })
}

/// A compiled spec wrapped in a `DeclarativeEngine` over the config's
/// egress for its id (a `[[engines]]` `[engines.<id>.egress]` table when
/// present, else a direct client — an unloadable config is not fatal for
/// a canary) plus the runtime driving its fetches.
fn live_engine(spec: CompiledSpec) -> Result<(DeclarativeEngine, tokio::runtime::Runtime), String> {
    let egress = Config::load().ok().and_then(|cfg| {
        cfg.engine(spec.id().as_str())
            .and_then(|e| e.egress.clone())
    });
    let http = HttpClient::from_egress_config(spec.id().clone(), egress.as_ref())
        .map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    Ok((DeclarativeEngine::new(spec, http), rt))
}

/// Bare `--live` (W3-06): the drift canary. `spec` is `None` when no
/// `<spec.yaml>` positional was given — then every embedded spec plus any
/// `$config_dir/engines/` override is canaried via [`load_specs_report`],
/// the strict loader: a spec that cannot even compile is itself drift and
/// reports as a failure rather than `load_specs`' silent skip. Each spec
/// re-fetches its baseline fixture's recorded query on page 1 (and page 2
/// when the spec paginates); every per-spec problem — load error,
/// unreadable/missing fixture, engine setup — prints `FAIL <id>` and the
/// loop continues, so one bad spec can never hide the others' results.
/// The run exits 1 when anything failed — the failed nightly run itself
/// is the report.
fn run_canary(
    spec: Option<CompiledSpec>,
    opts: &EngineArgs,
    config_dir: &Path,
    env: &cauce_core::config::EnvMap,
) -> Result<i32, String> {
    let sources = match spec {
        Some(spec) => vec![SpecSource {
            name: spec.id().to_string(),
            spec: Ok(spec),
        }],
        None => load_specs_report(config_dir, env),
    };
    if sources.is_empty() {
        return Err(format!(
            "no specs to canary (none embedded, none under {})",
            config_dir.join("engines").display()
        ));
    }
    let count = sources.len();
    let mut failures = 0;
    for source in sources {
        let spec = match source.spec {
            Ok(spec) => spec,
            Err(e) => {
                failures += 1;
                println!("FAIL {}: {e}", source.name);
                continue;
            }
        };
        let id = spec.id().clone();
        let (fixture_name, baseline) = match canary::baseline(&opts.fixtures_dir, id.as_str()) {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                failures += 1;
                println!(
                    "FAIL {id}: no committed page-1 fixture under {}",
                    opts.fixtures_dir.join(id.as_str()).display()
                );
                continue;
            }
            Err(e) => {
                failures += 1;
                println!("FAIL {id}: cannot read baseline fixture: {e}");
                continue;
            }
        };
        let (engine, rt) = match live_engine(spec) {
            Ok(pair) => pair,
            Err(e) => {
                failures += 1;
                println!("FAIL {id}: {e}");
                continue;
            }
        };
        let fetch = |req: &SearchRequest| rt.block_on(engine.fetch(req, LIVE_BUDGET));
        let report = canary::run(engine.compiled(), &baseline, &fetch);
        print_report(&id, &fixture_name, &report);
        if !report.ok() {
            failures += 1;
        }
    }
    println!("{count} spec(s) canaried, {failures} failed");
    Ok(if failures == 0 { 0 } else { 1 })
}

fn print_report(id: &EngineId, fixture: &str, report: &CanaryReport) {
    if report.ok() {
        let page2 = report
            .page2_count
            .map(|n| format!(", page 2: {n}"))
            .unwrap_or_default();
        println!(
            "PASS {id} [{fixture}] (page 1: {} results{page2})",
            report.page1_count
        );
    }
    for f in &report.failures {
        println!("FAIL {id} [{fixture}]: {f}");
    }
    for n in &report.notes {
        println!("note {id} [{fixture}]: {n}");
    }
}

/// `--live` without a query is the W3-06 canary; with one it is the
/// original fetch-once-and-print.
enum Live {
    /// Bare `--live`: drift canary over the committed fixture's query.
    Canary,
    /// `--live "<query>"`: fetch once, print parsed results.
    Query(String),
}

struct EngineArgs {
    /// `<spec.yaml>`; `None` only for the all-specs canary.
    spec: Option<PathBuf>,
    live: Option<Live>,
    record: bool,
    fixtures_dir: PathBuf,
}

/// `Ok(None)` is `-h`/`--help`: usage goes to stdout with exit 0, not
/// through the error path.
fn parse(args: &[String]) -> Result<Option<EngineArgs>, String> {
    // `cauce engine -h` asks for help before the `test` gate, same as
    // `cauce engine test -h` inside the flag loop below.
    if matches!(
        args.first().map(String::as_str),
        Some("-h") | Some("--help")
    ) {
        return Ok(None);
    }
    let mut it = args.iter().peekable();
    if it.next().map(String::as_str) != Some("test") {
        return Err("expected `cauce engine test <spec.yaml>`".to_string());
    }
    let mut spec = None;
    let mut live = None;
    let mut record = false;
    let mut fixtures_dir = PathBuf::from("engines/fixtures");
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        match flag {
            "--live" => {
                live = Some(match inline {
                    Some(query) => Live::Query(query),
                    // The value is optional: `--live` followed by a flag or
                    // nothing is the canary, `--live <query>` fetches that
                    // query. A non-flag token here is the query, not a
                    // positional — the spec positional must precede flags.
                    None if it.peek().is_some_and(|n| !n.starts_with('-')) => {
                        Live::Query(it.next().expect("peeked").clone())
                    }
                    None => Live::Canary,
                });
            }
            "--record" => record = true,
            "--fixtures-dir" => fixtures_dir = PathBuf::from(value(&mut it, &inline, flag)?),
            "-h" | "--help" => return Ok(None),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag {other:?}"));
            }
            positional => {
                if spec.replace(PathBuf::from(positional)).is_some() {
                    return Err("only one <spec.yaml> positional allowed".to_string());
                }
            }
        }
    }
    let spec = match spec {
        Some(s) => Some(s),
        // The canary is the only mode allowed to omit `<spec.yaml>` — it
        // then iterates every embedded spec.
        None if matches!(live, Some(Live::Canary)) => None,
        None => return Err("missing <spec.yaml>".to_string()),
    };
    if record && !matches!(live, Some(Live::Query(_))) {
        return Err("--record needs --live \"<query>\" (the fetch it records)".to_string());
    }
    Ok(Some(EngineArgs {
        spec,
        live,
        record,
        fixtures_dir,
    }))
}

/// Flag value: `--flag=value` inline or the next arg.
fn value(
    it: &mut Peekable<Iter<'_, String>>,
    inline: &Option<String>,
    name: &str,
) -> Result<String, String> {
    inline
        .clone()
        .or_else(|| it.next().cloned())
        .ok_or_else(|| format!("missing value for {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bare_live_is_the_canary() {
        let opts = parse(&args(&["test", "engines/bing.yaml", "--live"]))
            .unwrap()
            .unwrap();
        assert!(matches!(opts.live, Some(Live::Canary)));
        assert!(opts.spec.is_some());
    }

    #[test]
    fn bare_live_without_spec_canaries_every_embedded_spec() {
        let opts = parse(&args(&["test", "--live"])).unwrap().unwrap();
        assert!(matches!(opts.live, Some(Live::Canary)));
        assert!(opts.spec.is_none());
    }

    #[test]
    fn live_with_query_stays_fetch_once() {
        let opts = parse(&args(&["test", "s.yaml", "--live", "a b c"]))
            .unwrap()
            .unwrap();
        match opts.live {
            Some(Live::Query(q)) => assert_eq!(q, "a b c"),
            _ => panic!("expected Query"),
        }
        let opts = parse(&args(&["test", "s.yaml", "--live=a b"]))
            .unwrap()
            .unwrap();
        assert!(matches!(opts.live, Some(Live::Query(_))));
    }

    #[test]
    fn live_before_flags_still_reads_the_canary() {
        // `--live` followed by a flag keeps the value unset.
        let opts = parse(&args(&["test", "--live", "--fixtures-dir", "x"]))
            .unwrap()
            .unwrap();
        assert!(matches!(opts.live, Some(Live::Canary)));
        assert_eq!(opts.fixtures_dir, PathBuf::from("x"));
    }

    #[test]
    fn missing_spec_still_errors_outside_the_canary() {
        assert_eq!(
            parse(&args(&["test"])).err().as_deref(),
            Some("missing <spec.yaml>")
        );
        assert!(parse(&args(&["test", "--live", "q"])).is_err());
    }

    #[test]
    fn record_needs_a_query() {
        assert!(parse(&args(&["test", "--live", "--record"])).is_err());
        assert!(parse(&args(&["test", "s.yaml", "--record"])).is_err());
        assert!(parse(&args(&["test", "s.yaml", "--live", "q", "--record"])).is_ok());
    }
}
