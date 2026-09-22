//! `oxe engine test <spec.yaml>`: run the declarative spec's fixture pairs
//! offline (`engines/fixtures/<id>/<name>.html|.json` +
//! `<name>.expected.json`), `--live "<query>"` to fetch once and print the
//! parsed results, `--record` (with `--live`) to write a new fixture pair.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::time::Duration;

use oxe_core::config::{Config, system_env};
use oxe_core::http::HttpClient;
use oxe_core::{ClientKind, Engine, SafeSearch, SearchRequest};
use oxe_engines::declarative::fixtures::{
    FixtureError, FixtureReport, compile_spec_source, fixture_pairs, run_pair, write_pair,
};
use oxe_engines::declarative::{CompiledSpec, DeclarativeEngine};

const USAGE: &str = "usage: oxe engine test <spec.yaml> [--live \"<query>\"] [--record] [--fixtures-dir <engines/fixtures>]";

/// Budget for one `--live` fetch (capped further by the spec's
/// `request.timeout_ms`).
const LIVE_BUDGET: Duration = Duration::from_secs(30);

/// Entry point for the `engine` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match parse(args).and_then(dispatch) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("oxe engine: {msg}\n{USAGE}");
            2
        }
    }
}

fn dispatch(opts: EngineArgs) -> Result<i32, String> {
    let spec = compile_spec_source(
        &opts.spec,
        &oxe_core::config::Dirs::detect().config_dir,
        &system_env(),
    )
    .map_err(|e| e.to_string())?;
    match &opts.live {
        Some(query) => run_live(spec, query, &opts),
        None => run_fixtures(&spec, &opts),
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

/// `--live`: one real fetch through the engine's configured egress;
/// `--record` additionally writes the fixture pair.
fn run_live(spec: CompiledSpec, query: &str, opts: &EngineArgs) -> Result<i32, String> {
    // A `[[engines]]` entry for this id may carry an `[engines.<id>.egress]`
    // table; an unloadable config is not fatal for a canary — fall back to
    // a direct client.
    let egress = Config::load().ok().and_then(|cfg| {
        cfg.engine(spec.id().as_str())
            .and_then(|e| e.egress.clone())
    });
    let http = HttpClient::from_egress_config(spec.id().clone(), egress.as_ref())
        .map_err(|e| e.to_string())?;
    let engine = DeclarativeEngine::new(spec, http);

    let req = SearchRequest {
        q: query.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: Some(vec![engine.id()]),
        client: ClientKind::Cli,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let fetched = rt
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

struct EngineArgs {
    spec: PathBuf,
    live: Option<String>,
    record: bool,
    fixtures_dir: PathBuf,
}

fn parse(args: &[String]) -> Result<EngineArgs, String> {
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("test") {
        return Err("expected `oxe engine test <spec.yaml>`".to_string());
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
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("missing value for {name}"))
        };
        match flag {
            "--live" => live = Some(value("--live")?),
            "--record" => record = true,
            "--fixtures-dir" => fixtures_dir = PathBuf::from(value("--fixtures-dir")?),
            "-h" | "--help" => return Err("help requested".into()),
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
    let spec = spec.ok_or("missing <spec.yaml>")?;
    if record && live.is_none() {
        return Err("--record needs --live \"<query>\" (the fetch it records)".to_string());
    }
    Ok(EngineArgs {
        spec,
        live,
        record,
        fixtures_dir,
    })
}
