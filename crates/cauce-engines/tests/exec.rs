//! Integration tests for the `exec` engine runtime against real `python3`
//! children running `cauce_engine_sdk` from `sdk/python`.
//!
//! These tests are skipped when `python3` is not on PATH. The ddgs canary is
//! additionally gated behind `CAUCE_LIVE=1` and needs the `ddgs` extra
//! installed (`uv pip install ddgs`, or `uv sync --extra ddgs` in
//! `sdk/python` and a matching interpreter).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cauce_core::{
    ClientKind, Engine, EngineError, EngineId, SafeSearch, SearchRequest, SearchResult, Tier,
    TimeRange,
};
use cauce_engines::exec::{ExecEngine, ExecSpec};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate dir has two ancestors")
        .to_path_buf()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/echo_engine.py")
}

fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn echo_spec(extra_args: &[&str]) -> ExecSpec {
    let mut args = vec![fixture().to_string_lossy().into_owned()];
    args.extend(extra_args.iter().map(|s| (*s).to_string()));
    ExecSpec {
        id: EngineId::new("echo"),
        command: "python3".to_string(),
        args,
        env: vec![],
        cwd: None,
        page_size: 10,
        tier: Tier::T2,
        params: Default::default(),
    }
}

fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: Some("en".to_string()),
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Api,
    }
}

/// A v1-only child: rejects `v != 1` with the reference SDK's
/// `parse:unsupported protocol version` error (issue #88 downgrade path).
/// `extra_args` are forwarded to the fixture (`--bare-rejection` sends an
/// out-of-contract rejection line with no `v` field).
fn v1_spec(extra_args: &[&str]) -> ExecSpec {
    let mut spec = echo_spec(&[]);
    spec.args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/v1_engine.py")
            .to_string_lossy()
            .into_owned(),
    ];
    spec.args
        .extend(extra_args.iter().map(|s| (*s).to_string()));
    spec
}

/// The fixture tags every snippet with `pid=<n>`; used to prove respawns.
fn pid_of(results: &[SearchResult]) -> u32 {
    results[0]
        .snippet
        .strip_prefix("pid=")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|pid| pid.parse().ok())
        .expect("fixture snippet carries pid=<n>")
}

#[tokio::test]
async fn exec_round_trip() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_round_trip");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&[]));
    let res = engine
        .search(&req("hello world"), Duration::from_secs(10))
        .await
        .expect("round trip succeeds");
    assert_eq!(res.len(), 3);
    assert!(res[0].title.contains("hello world"));
    assert_eq!(res[0].engine, EngineId::new("echo"));
    assert_eq!(res[0].url.host_str(), Some("example.com"));
}

#[tokio::test]
async fn exec_respawns_after_crash() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_respawns_after_crash");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&["--crash-after", "1"]));

    let r1 = engine
        .search(&req("first"), Duration::from_secs(10))
        .await
        .expect("first call answers");
    let pid1 = pid_of(&r1);

    // The child exits when the second request arrives, without answering.
    let err = engine
        .search(&req("second"), Duration::from_secs(10))
        .await
        .expect_err("call into the dying child fails");
    assert!(
        matches!(err, EngineError::Transport(_)),
        "expected Transport, got {err:?}"
    );

    // The next call runs against a fresh process.
    let r3 = engine
        .search(&req("third"), Duration::from_secs(10))
        .await
        .expect("respawned child answers");
    assert_ne!(pid_of(&r3), pid1, "child was respawned");
}

#[tokio::test]
async fn exec_kills_child_at_deadline() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_kills_child_at_deadline");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&["--sleep", "3", "--sleep-on", "slow"]));

    let err = engine
        .search(&req("slow query"), Duration::from_millis(300))
        .await
        .expect_err("sleeping child overruns the budget");
    assert_eq!(err, EngineError::Timeout);

    // The child was killed and respawned: a fast query answers inside the
    // budget, and the answer is for *this* query (a stale line from the old
    // process would carry the old query text).
    let res = engine
        .search(&req("fast"), Duration::from_secs(10))
        .await
        .expect("fresh child answers");
    assert!(
        res[0].title.contains("fast"),
        "expected results for the new query, got {res:?}"
    );
}

/// The pipeline wraps `engine.search` in its own `tokio::time::timeout`, so
/// the outer deadline wins and the search future is *dropped* mid-round-trip
/// while a request is still in flight on the child. Dropping must reap the
/// child: if the cancelled child stayed in `state`, it would write its stale
/// response later, and the next `search` would read that line as the answer
/// to its own (different) query — protocol v1 has no correlation field, so
/// the wrong query's results would be cached under the new query's key.
#[tokio::test]
async fn exec_cancelled_call_does_not_poison_next_search() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_cancelled_call_does_not_poison_next_search");
        return;
    }
    // Sleeps 1 s before answering any query containing "slow".
    let engine = ExecEngine::new(echo_spec(&["--sleep", "1", "--sleep-on", "slow"]));

    // Outer deadline drops the in-flight future; the engine's own 30 s
    // budget never gets a chance to fire.
    let cancelled = tokio::time::timeout(
        Duration::from_millis(200),
        engine.search(&req("slow query"), Duration::from_secs(30)),
    )
    .await;
    assert!(
        cancelled.is_err(),
        "outer deadline drops the in-flight call"
    );

    // Let the abandoned child finish its sleep and write the stale line;
    // the fixture stays alive afterwards, so a buggy implementation would
    // still find it "alive" and reuse it.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let res = tokio::time::timeout(
        Duration::from_secs(10),
        engine.search(&req("fast"), Duration::from_secs(10)),
    )
    .await
    .expect("second call completes")
    .expect("fresh child answers");
    assert!(
        res[0].title.contains("fast"),
        "stale response from the cancelled call poisoned this search: {res:?}"
    );
}

/// Issue #83, first half: an engine built inside a tokio runtime (the
/// `cauce serve`/`cauce mcp` factory path) spawns its child eagerly, so the
/// first request's deadline is not spent on the child's boot. A boot delay
/// longer than the budget proves it: this search could only succeed if the
/// child was already started before the request arrived.
#[tokio::test]
async fn exec_spawns_eagerly_inside_a_runtime() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_spawns_eagerly_inside_a_runtime");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&["--boot-delay", "1.5"]));
    // Let the eagerly spawned child finish its boot delay.
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let res = engine
        .search(&req("warm"), Duration::from_millis(600))
        .await
        .expect("pre-booted child answers inside a tight budget");
    assert_eq!(res.len(), 3);
}

/// The cold counterpart of the eager spawn: built outside a runtime the
/// child is lazy and a tight first-request budget dies on the boot cost.
/// This was the issue #83 repro at every request; it now only describes the
/// lazy fallback path (engines built before a runtime exists).
#[test]
fn exec_lazy_spawn_burns_first_request_budget() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_lazy_spawn_burns_first_request_budget");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&["--boot-delay", "1.5"]));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let err = rt
        .block_on(engine.search(&req("cold"), Duration::from_millis(500)))
        .expect_err("cold boot overruns the budget");
    assert_eq!(err, EngineError::Timeout);
}

/// Issue #83, second half: when a request's deadline kills the child
/// mid-query (here through the pipeline's own pattern, an outer
/// `tokio::time::timeout` that drops the in-flight `search` future) the
/// engine re-warms immediately instead of leaving the next caller to
/// cold-start inside its own deadline. The replacement boots during the
/// gap between requests, so a budget smaller than the boot delay suffices.
#[tokio::test]
async fn exec_deadline_kill_rewarms_for_next_request() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_deadline_kill_rewarms_for_next_request");
        return;
    }
    let engine = ExecEngine::new(echo_spec(&[
        "--boot-delay",
        "1.5",
        "--sleep",
        "3",
        "--sleep-on",
        "stall",
    ]));
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let r1 = engine
        .search(&req("first"), Duration::from_secs(5))
        .await
        .expect("warm child answers");
    let pid1 = pid_of(&r1);

    // The outer deadline drops the in-flight future; kill_on_drop reaps
    // the child and the drop path kicks off an eager respawn.
    let dropped = tokio::time::timeout(
        Duration::from_millis(300),
        engine.search(&req("stall"), Duration::from_secs(30)),
    )
    .await;
    assert!(dropped.is_err(), "outer deadline drops the in-flight call");

    // Once the respawned child has had its boot window, the next request
    // must not be paying cold start: a budget below the boot delay still
    // answers.
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let res = engine
        .search(&req("next"), Duration::from_millis(600))
        .await
        .expect("re-warmed child answers inside a tight budget");
    assert_ne!(pid_of(&res), pid1, "child was respawned");
    assert!(
        res[0].title.contains("next"),
        "expected results for the new query, got {res:?}"
    );
}

/// A `command` that does not exist must not fail construction or startup:
/// the eager spawn degrades to lazy, and the first `search` surfaces the
/// spawn error as `Transport`.
#[tokio::test]
async fn exec_missing_command_degrades_to_lazy() {
    let mut spec = echo_spec(&[]);
    spec.command = "cauce-no-such-binary-83".to_string();
    spec.args = vec![];
    let engine = ExecEngine::new(spec); // must not panic inside a runtime
    let err = engine
        .search(&req("q"), Duration::from_secs(5))
        .await
        .expect_err("lazy spawn surfaces the missing binary");
    assert!(
        matches!(err, EngineError::Transport(_)),
        "expected Transport, got {err:?}"
    );
}

#[test]
fn sdk_malformed_lines_get_error_responses() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping sdk_malformed_lines_get_error_responses");
        return;
    }
    let mut child = Command::new("python3")
        .arg(fixture())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fixture");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Garbage in -> error response line, process stays alive.
    writeln!(stdin, "this is not json").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).expect("response parses");
    // No request version to echo on a parse failure: PROTOCOL_VERSION (v2).
    assert_eq!(v["v"], 2);
    assert!(v["error"].is_string(), "malformed input yields an error");
    assert!(v["results"].as_array().unwrap().is_empty());
    assert!(child.try_wait().unwrap().is_none(), "child still alive");

    // A well-formed request after the garbage still answers.
    stdin
        .write_all(br#"{"v":1,"query":"ok","page":1,"lang":"en","timeout_ms":5000}"#)
        .unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).expect("response parses");
    assert!(v["error"].is_null());
    assert_eq!(v["results"].as_array().unwrap().len(), 3);

    // EOF -> clean exit.
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn sdk_python_unit_tests_pass() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping sdk_python_unit_tests_pass");
        return;
    }
    let test_file = repo_root().join("sdk/python/tests/test_sdk.py");
    let status = Command::new("python3")
        .arg(&test_file)
        .status()
        .expect("run python sdk unit tests");
    assert!(status.success(), "python sdk unit tests failed");
}

/// Issue #88: a v2 child receives `safesearch`, `time_range` and the
/// spec's static `params` on the request line. The echo fixture tags its
/// snippet with what it parsed.
#[tokio::test]
async fn exec_v2_forwards_safesearch_time_range_and_params() {
    if !have_python3() {
        eprintln!(
            "python3 not on PATH; skipping exec_v2_forwards_safesearch_time_range_and_params"
        );
        return;
    }
    let mut spec = echo_spec(&[]);
    spec.params
        .insert("region".to_string(), "wt-wt".to_string());
    let engine = ExecEngine::new(spec);

    let mut request = req("params check");
    request.safesearch = SafeSearch::Strict;
    request.time_range = Some(TimeRange::Week);
    let res = engine
        .search(&request, Duration::from_secs(10))
        .await
        .expect("round trip succeeds");
    let snippet = &res[0].snippet;
    assert!(snippet.contains("v=2"), "child saw v2 request: {snippet}");
    assert!(
        snippet.contains("safesearch=strict"),
        "safesearch forwarded: {snippet}"
    );
    assert!(
        snippet.contains("time_range=week"),
        "time_range forwarded: {snippet}"
    );
    assert!(
        snippet.contains("region"),
        "spec params forwarded: {snippet}"
    );
}

/// Issue #88, negotiation half: a v1-only child rejects the optimistic v2
/// request, the parent downgrades that process to v1 and resends without the
/// v2 fields. The fixture reports which v2-only fields leaked onto the wire.
#[tokio::test]
async fn exec_v1_child_downgrades_and_omits_v2_fields() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_v1_child_downgrades_and_omits_v2_fields");
        return;
    }
    let engine = ExecEngine::new(v1_spec(&[]));

    let mut request = req("downgrade");
    request.safesearch = SafeSearch::Strict;
    request.time_range = Some(TimeRange::Day);
    let res = engine
        .search(&request, Duration::from_secs(10))
        .await
        .expect("v1 child answers after downgrade");
    assert!(
        res[0].title.contains("downgrade"),
        "expected results for the resent query: {res:?}"
    );
    assert!(
        res[0].snippet.contains("leaked_fields=[]"),
        "v2 fields must be omitted on the resent v1 request: {res:?}"
    );

    // The downgrade is cached on the process: the next call answers without
    // another rejected probe.
    let res = engine
        .search(&req("second"), Duration::from_secs(10))
        .await
        .expect("subsequent v1 call answers");
    assert!(res[0].title.contains("second"));
}

/// Issue #88 regression: a strict third-party v1 child whose rejection line
/// lacks `v` entirely (`{"error":"unsupported protocol version: 2"}`) fails
/// `ExecResponse` decode. The raw-substring fallback in
/// `is_version_rejection` must still catch it — without it this child
/// looped forever on Parse error, kill, respawn, re-probe v2.
#[tokio::test]
async fn exec_v1_child_bare_rejection_still_downgrades() {
    if !have_python3() {
        eprintln!("python3 not on PATH; skipping exec_v1_child_bare_rejection_still_downgrades");
        return;
    }
    let engine = ExecEngine::new(v1_spec(&["--bare-rejection"]));

    let mut request = req("bare downgrade");
    request.safesearch = SafeSearch::Off;
    request.time_range = Some(TimeRange::Month);
    let res = engine
        .search(&request, Duration::from_secs(10))
        .await
        .expect("v1 child answers after downgrade on a bare rejection");
    assert!(
        res[0].title.contains("bare downgrade"),
        "expected results for the resent query: {res:?}"
    );
    assert!(
        res[0].snippet.contains("leaked_fields=[]"),
        "v2 fields must be omitted on the resent v1 request: {res:?}"
    );
}

#[tokio::test]
async fn exec_ddgs_live() {
    if std::env::var_os("CAUCE_LIVE").is_none() {
        return; // live canary; skipped by default
    }
    assert!(have_python3(), "CAUCE_LIVE=1 but python3 is not on PATH");
    let has_ddgs = Command::new("python3")
        .args(["-c", "import ddgs"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(
        has_ddgs,
        "CAUCE_LIVE=1 but `import ddgs` failed; install the ddgs extra (uv pip install ddgs)"
    );
    let engine = ExecEngine::new(ExecSpec::ddgs(repo_root()));
    let res = engine
        .search(&req("tanstack router"), Duration::from_secs(30))
        .await
        .expect("ddgs live search succeeds");
    assert!(!res.is_empty(), "ddgs live returned zero results");
}
