//! `exec` engine runtime: one warm child process per engine, JSON lines over
//! stdin/stdout (parent plan section 4.3, protocol `v: 2`).
//!
//! Protocol v2 adds `safesearch`, `time_range` and a per-engine `params`
//! object to the request line (issue #88); v1 children never see them.
//! Negotiation is optimistic per process: a fresh child gets a v2 request and
//! an `error` naming the protocol version (the v1 reference SDK answers
//! `parse:unsupported protocol version: 2`) downgrades that child to v1 and
//! resends. v2 children must accept `v: 1` requests — a strict subset — and
//! echo the request's `v`, so old parents keep working against new children.
//!
//! One request is in flight per process; concurrent `search` calls serialize on
//! a mutex. The child is spawned eagerly when the engine is built inside a
//! tokio runtime (the `serve`/`mcp` factory path) so the first request is not
//! racing process boot against its deadline (issue #83); built outside a
//! runtime, or after a failed eager spawn, it degrades to lazy spawn on the
//! first call. A child that crashes or overruns the request budget is killed
//! and re-warmed immediately, including when the pipeline's outer deadline
//! *drops* the in-flight `search`, so the next call never cold-starts inside
//! its own deadline. Child stderr is forwarded to `tracing` at `warn`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, warn};

use cauce_core::{
    Engine, EngineError, EngineId, EnginePhase, Metrics, SafeSearch, SearchRequest, SearchResult,
    Tier, TimeRange,
};

/// Version of the exec wire protocol implemented here (parent plan 4.3).
pub const PROTOCOL_VERSION: u8 = 2;

/// Oldest protocol version a child may speak. v1 predates the
/// `safesearch`/`time_range`/`params` request fields (issue #88).
pub const MIN_PROTOCOL_VERSION: u8 = 1;

/// Static description of an exec engine: how to spawn it and where it sits in
/// the fan-out tiers.
#[derive(Debug, Clone)]
pub struct ExecSpec {
    /// Stable engine id (`ddgs`, ...).
    pub id: EngineId,
    /// Executable to spawn (`python3`, ...).
    pub command: String,
    /// Arguments passed to `command`.
    pub args: Vec<String>,
    /// Extra environment on top of the inherited one.
    pub env: Vec<(String, String)>,
    /// Working directory of the child; `None` keeps the parent's.
    pub cwd: Option<PathBuf>,
    /// Results per page the engine reports back (ddgs: 10).
    pub page_size: u8,
    /// Fan-out tier (parent plan 4.4).
    pub tier: Tier,
    /// Static per-engine params forwarded on every v2 request (from the
    /// `[engines.params]` config table). Omitted on the wire when empty or
    /// when the child negotiated v1.
    pub params: BTreeMap<String, String>,
}

impl ExecSpec {
    /// The day-1 bridge from the wave-0 plan
    /// (`[[engines]] id="ddgs" kind="exec" command="python3"
    /// args=["sdk/python/cauce_engine_sdk/ddgs_auto.py"]`, tier 2, page_size 10).
    ///
    /// `cwd` is the directory the relative script arg resolves against,
    /// typically the repo or install root.
    pub fn ddgs(cwd: PathBuf) -> Self {
        Self {
            id: EngineId::new("ddgs"),
            command: "python3".to_string(),
            args: vec!["sdk/python/cauce_engine_sdk/ddgs_auto.py".to_string()],
            env: vec![],
            cwd: Some(cwd),
            page_size: 10,
            tier: Tier::T2,
            params: BTreeMap::new(),
        }
    }
}

/// Outbound request line (`->` on the child's stdin).
///
/// Field names are the settled wire contract: `query`, `page`, `lang`,
/// `timeout_ms`, plus the v2 additions `safesearch`, `time_range` and
/// `params` (issue #88). The v2 fields are `None`/empty — and therefore
/// absent from the JSON — on a v1-negotiated child, so strict v1 decoders
/// keep working.
#[derive(Debug, Clone, Serialize)]
pub struct ExecRequest {
    pub v: u8,
    pub query: String,
    pub page: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    pub timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safesearch: Option<SafeSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl ExecRequest {
    fn new(req: &SearchRequest, budget: Duration, version: u8, spec: &ExecSpec) -> Self {
        let v2 = version >= 2;
        Self {
            v: version,
            query: req.q.clone(),
            page: req.page,
            lang: req.lang.clone(),
            timeout_ms: budget.as_millis().clamp(1, u64::MAX as u128) as u64,
            safesearch: v2.then_some(req.safesearch),
            time_range: if v2 { req.time_range } else { None },
            params: if v2 {
                spec.params.clone()
            } else {
                BTreeMap::new()
            },
        }
    }
}

/// One result row inside an [`ExecResponse`].
#[derive(Debug, Clone, Deserialize)]
pub struct ExecResultRow {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub snippet: String,
}

/// Inbound response line (`<-` on the child's stdout). `v` echoes the
/// request's version, so a negotiated-v1 child answers `v: 1`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecResponse {
    pub v: u8,
    #[serde(default)]
    pub results: Vec<ExecResultRow>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Does a raw response line reject the request's protocol version? The v1
/// reference SDK answers `parse:unsupported protocol version: 2`; any error
/// naming the protocol version is treated the same so strict third-party v1
/// children downgrade too. One line in, one line out: the stream stays in
/// sync and the request can be resent at the lower version.
///
/// Two checks: the structured one reads `error` off a decoded
/// [`ExecResponse`]; when the line does not decode at all — e.g. an
/// out-of-contract child sends `{"error":"unsupported protocol version"}`
/// with no `v` — a raw substring fallback still catches the intent. Without
/// the fallback such a child would loop forever: Parse error, kill, respawn,
/// re-probe v2.
fn is_version_rejection(line: &str) -> bool {
    let trimmed = line.trim();
    match serde_json::from_str::<ExecResponse>(trimmed) {
        Ok(resp) => resp
            .error
            .as_deref()
            .is_some_and(|e| e.to_lowercase().contains("protocol version")),
        Err(_) => trimmed.to_lowercase().contains("protocol version"),
    }
}

/// Map a protocol `error` string to an `EngineError`. Recognised codes are
/// `rate_limited`, `blocked`, `no_results`, `timeout` and `parse[:detail]`;
/// anything else (including `transport:<detail>`) becomes `Transport`.
fn map_protocol_error(err: &str) -> EngineError {
    let (code, detail) = err.split_once(':').unwrap_or((err, ""));
    match code.trim() {
        "rate_limited" => EngineError::RateLimited,
        "blocked" => EngineError::Blocked,
        "no_results" => EngineError::NoResults,
        "timeout" => EngineError::Timeout,
        "parse" => EngineError::Parse(detail.trim().to_string()),
        _ => EngineError::Transport(err.to_string()),
    }
}

/// Live pipes of one spawned child.
struct ChildIo {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Negotiated protocol version for this process: optimistic
    /// [`PROTOCOL_VERSION`] at spawn, downgraded on a version-rejection
    /// response (issue #88). Cached here so the probe costs a v1 child one
    /// extra round trip per process lifetime.
    version: u8,
}

#[derive(Default)]
struct State {
    child: Option<ChildIo>,
}

/// Spawn the child. Must be called inside a tokio runtime (the stderr
/// forwarder is a spawned task).
fn spawn_child(spec: &ExecSpec) -> Result<ChildIo, EngineError> {
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args)
        .envs(spec.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| EngineError::Transport(format!("spawn `{}` failed: {e}", spec.command)))?;
    if let Some(stderr) = child.stderr.take() {
        let id = spec.id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!(engine = %id, "exec child stderr: {line}");
            }
        });
    }
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| EngineError::Transport("child stdin not piped".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::Transport("child stdout not piped".into()))?;
    debug!(engine = %spec.id, "exec child spawned");
    Ok(ChildIo {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        version: PROTOCOL_VERSION,
    })
}

/// Re-warm the engine after a kill (issue #83). Armed for the span of one
/// in-flight request: every path that ends without putting a live child back
/// into `state` (the engine's own deadline, an io failure, a decode desync,
/// or the pipeline's outer deadline dropping the whole `search` future)
/// leaves `state.child` empty and a dead process behind. `Drop` then spawns
/// the replacement on a detached task so the next caller finds a child that
/// is already booted or booting, instead of paying cold-start inside its own
/// deadline and dying the same way. A failed respawn leaves `child` at `None`
/// and the next `search` retries lazily, so a broken `command` cannot hot
/// loop here.
struct RespawnOnDrop {
    spec: ExecSpec,
    state: Arc<Mutex<State>>,
    armed: bool,
}

impl RespawnOnDrop {
    fn armed(spec: &ExecSpec, state: &Arc<Mutex<State>>) -> Self {
        Self {
            spec: spec.clone(),
            state: Arc::clone(state),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RespawnOnDrop {
    fn drop(&mut self) {
        // `Handle::try_current` because this also runs when the runtime is
        // tearing down: no respawn is worth a panic in a destructor, the next
        // call simply cold-spawns as before.
        if !self.armed || Handle::try_current().is_err() {
            return;
        }
        let spec = self.spec.clone();
        let state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let mut state = state.lock().await;
            // `None` can also mean a concurrent `search` holds the child
            // right now; installing a spare then is harmless, it is either
            // adopted by the next call or killed by `kill_on_drop` when the
            // in-flight call puts its own child back.
            if state.child.is_none() {
                match spawn_child(&spec) {
                    Ok(io) => {
                        debug!(engine = %spec.id, "exec child re-warmed after kill");
                        state.child = Some(io);
                    }
                    Err(e) => warn!(engine = %spec.id, error = %e,
                        "eager respawn failed; next call retries lazily"),
                }
            }
        });
    }
}

/// A warm `exec` engine: owns one child process and speaks protocol v2 to it
/// (negotiated down to v1 per process when the child rejects v2, issue #88).
///
/// The child is spawned eagerly when `new` runs inside a tokio runtime (the
/// `serve`/`mcp` factory path, tests); a failed spawn degrades to the lazy
/// path, so `new` is safe to call outside a runtime and never fails on a
/// missing `command`.
pub struct ExecEngine {
    spec: ExecSpec,
    state: Arc<Mutex<State>>,
    /// W1-09 phase timings (`cauce_engine_duration_ms{phase}`): `http` is the
    /// stdin/stdout round trip, `parse` the response decode.
    metrics: Metrics,
}

impl ExecEngine {
    pub fn new(spec: ExecSpec) -> Self {
        let mut state = State::default();
        // Eager warm spawn (issue #83): pay the child's boot cost at
        // construction instead of inside the first request's deadline.
        // `spawn_child` needs a runtime (stderr forwarder task); without
        // one, or on a spawn error, the first `search` spawns lazily and
        // surfaces the error there.
        if Handle::try_current().is_ok() {
            match spawn_child(&spec) {
                Ok(io) => state.child = Some(io),
                Err(e) => warn!(engine = %spec.id, error = %e,
                    "eager spawn failed; first search retries lazily"),
            }
        }
        Self {
            spec,
            state: Arc::new(Mutex::new(state)),
            metrics: Metrics,
        }
    }

    pub fn spec(&self) -> &ExecSpec {
        &self.spec
    }

    /// Take the live child out of `state`, respawning first when the
    /// previous one exited. The caller owns the `ChildIo` for the round
    /// trip and puts it back into `state` only on a fully successful call:
    /// while it is out, a dropped `search` future drops the `ChildIo` too,
    /// and `kill_on_drop` (set in `spawn`) reaps the process. This is what
    /// keeps a cancelled call from leaving an unanswered request pending on
    /// a child the next `search` would reuse — protocol v1 has no request
    /// correlation, so a stale response would decode cleanly as the next
    /// query's answer (cross-query cache poisoning).
    async fn ensure_child(&self, state: &mut State) -> Result<ChildIo, EngineError> {
        let dead = match state.child.as_mut() {
            None => true,
            Some(io) => match io.child.try_wait() {
                Ok(None) => false,
                Ok(Some(status)) => {
                    warn!(engine = %self.spec.id, %status, "exec child exited; respawning");
                    true
                }
                Err(e) => {
                    warn!(engine = %self.spec.id, "exec child wait failed ({e}); respawning");
                    true
                }
            },
        };
        if dead {
            if let Some(mut io) = state.child.take() {
                let _ = io.child.kill().await; // reap the zombie
            }
            state.child = Some(spawn_child(&self.spec)?);
        }
        Ok(state.child.take().expect("child present"))
    }

    fn decode(&self, line: &str) -> Result<Vec<SearchResult>, EngineError> {
        let resp: ExecResponse = serde_json::from_str(line.trim())
            .map_err(|e| EngineError::Parse(format!("bad exec response line: {e}")))?;
        // The response `v` echoes the request's, so a negotiated-v1 child
        // legitimately answers `v: 1`.
        if !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&resp.v) {
            return Err(EngineError::Transport(format!(
                "exec protocol version {} unsupported",
                resp.v
            )));
        }
        if let Some(err) = resp.error.filter(|e| !e.is_empty()) {
            return Err(map_protocol_error(&err));
        }
        let mut out = Vec::with_capacity(resp.results.len());
        for row in resp.results {
            match url::Url::parse(&row.url) {
                Ok(url) => out.push(SearchResult {
                    url,
                    title: row.title,
                    snippet: row.snippet,
                    engine: self.spec.id.clone(),
                    published: None,
                    score: 0.0,
                }),
                Err(e) => {
                    debug!(engine = %self.spec.id, url = %row.url, "dropping bad result url: {e}");
                }
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl Engine for ExecEngine {
    fn id(&self) -> EngineId {
        self.spec.id.clone()
    }

    fn tier(&self) -> Tier {
        self.spec.tier
    }

    fn page_size(&self) -> u8 {
        self.spec.page_size
    }

    async fn search(
        &self,
        req: &SearchRequest,
        budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        // One request in flight per process: the lock is held for the whole
        // round trip. The child is owned by this future while the request
        // is in flight (see `ensure_child`), so cancelling the call — e.g.
        // the pipeline's outer deadline winning over `budget` — drops `io`
        // and reaps the process instead of leaving a stale response queued
        // for the next caller.
        let mut state = self.state.lock().await;
        let mut io = self.ensure_child(&mut state).await?;
        // From here every exit path kills the child this call is holding;
        // the guard re-warms `state` so the next request is not cold.
        let mut respawn = RespawnOnDrop::armed(&self.spec, &self.state);

        // Encode inside the round trip: when a v1 child rejects the v2
        // request the loop downgrades `io.version` and resends on the same
        // child — the rejection is a well-formed response line, so the
        // stream stays in sync (issue #88).
        let round_trip = async {
            loop {
                let mut line =
                    serde_json::to_string(&ExecRequest::new(req, budget, io.version, &self.spec))
                        .map_err(|e| EngineError::Parse(format!("request encode: {e}")))?;
                line.push('\n');
                io.stdin
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| EngineError::Transport(format!("child io: {e}")))?;
                io.stdin
                    .flush()
                    .await
                    .map_err(|e| EngineError::Transport(format!("child io: {e}")))?;
                let mut buf = String::new();
                io.stdout
                    .read_line(&mut buf)
                    .await
                    .map_err(|e| EngineError::Transport(format!("child io: {e}")))?;
                if buf.is_empty()
                    || io.version == MIN_PROTOCOL_VERSION
                    || !is_version_rejection(&buf)
                {
                    return Ok::<String, EngineError>(buf);
                }
                debug!(engine = %self.spec.id,
                    "exec child rejected protocol v{}; downgrading to v{MIN_PROTOCOL_VERSION}",
                    io.version);
                io.version = MIN_PROTOCOL_VERSION;
            }
        };

        // Every failure path returns early with `io` still owned here:
        // dropping it kills the child (`kill_on_drop`), `state.child` stays
        // `None`, and the next call respawns on a clean stream.
        let fetch = Instant::now();
        let buf = match timeout(budget, round_trip).await {
            Err(_) => {
                self.metrics
                    .record_engine_phase(&self.spec.id, EnginePhase::Http, fetch.elapsed());
                warn!(engine = %self.spec.id, budget_ms = budget.as_millis() as u64,
                    "exec engine deadline hit; killing child");
                return Err(EngineError::Timeout);
            }
            Ok(Err(e)) => {
                self.metrics
                    .record_engine_phase(&self.spec.id, EnginePhase::Http, fetch.elapsed());
                warn!(engine = %self.spec.id, "exec round trip failed ({e}); killing child");
                return Err(e);
            }
            Ok(Ok(buf)) if buf.is_empty() => {
                self.metrics
                    .record_engine_phase(&self.spec.id, EnginePhase::Http, fetch.elapsed());
                warn!(engine = %self.spec.id, "exec child closed stdout (EOF); will respawn");
                return Err(EngineError::Transport("engine process exited".into()));
            }
            Ok(Ok(buf)) => buf,
        };
        self.metrics
            .record_engine_phase(&self.spec.id, EnginePhase::Http, fetch.elapsed());

        let parse = Instant::now();
        let decoded = self.decode(&buf);
        self.metrics
            .record_engine_phase(&self.spec.id, EnginePhase::Parse, parse.elapsed());
        match decoded {
            Ok(results) => {
                respawn.disarm(); // a live child is going back into `state`
                state.child = Some(io);
                Ok(results)
            }
            // A response that fails to decode means the stream position is
            // untrustworthy (protocol desync); kill the child too.
            Err(e) => {
                warn!(engine = %self.spec.id, error = %e,
                    "exec response undecodable; killing child");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;
    use url::Url;

    use super::*;

    /// Outside a tokio runtime `ExecEngine::new` defers spawning, so this
    /// never launches the (nonexistent) command.
    fn engine() -> ExecEngine {
        ExecEngine::new(ExecSpec {
            id: EngineId::new("fuzz"),
            command: "nonexistent-cauce-fuzz".to_string(),
            args: vec![],
            env: vec![],
            cwd: None,
            page_size: 10,
            tier: Tier::T2,
            params: BTreeMap::new(),
        })
    }

    /// Error strings naming the protocol version in any casing.
    fn arb_version_err() -> impl Strategy<Value = String> {
        (
            ".*",
            prop::sample::select(vec![
                "protocol version",
                "Protocol Version",
                "PROTOCOL VERSION",
            ]),
            ".*",
        )
            .prop_map(|(a, m, b)| format!("{a}{m}{b}"))
    }

    proptest! {
        /// A raw response line of any shape never panics the
        /// version-rejection probe.
        #[test]
        fn is_version_rejection_never_panics(line in ".*") {
            let _ = is_version_rejection(&line);
        }

        /// Any error naming the protocol version downgrades — via the
        /// decoded `error` field or the raw substring fallback for
        /// undecodable lines.
        #[test]
        fn is_version_rejection_catches_version_errors(
            e in arb_version_err(),
            well_formed in any::<bool>(),
        ) {
            let line = if well_formed {
                json!({"v": PROTOCOL_VERSION, "error": e}).to_string()
            } else {
                e
            };
            prop_assert!(is_version_rejection(&line));
        }

        /// Arbitrary error strings never panic the mapper.
        #[test]
        fn map_protocol_error_never_panics(err in ".*") {
            let _ = map_protocol_error(&err);
        }

        /// Known codes map to their variant, `parse:` keeps the trimmed
        /// detail, and anything else is `Transport` with the raw error
        /// string verbatim.
        #[test]
        fn map_protocol_error_pins_codes(
            code in prop::sample::select(vec![
                "rate_limited",
                "blocked",
                "no_results",
                "timeout",
                "parse",
                "transport",
                "weird",
                "",
                "RATE_LIMITED",
            ]),
            detail in ".*",
            colon in any::<bool>(),
        ) {
            let err = if colon {
                format!("{code}:{detail}")
            } else {
                code.to_string()
            };
            let got = map_protocol_error(&err);
            match code.trim() {
                "rate_limited" => prop_assert_eq!(got, EngineError::RateLimited),
                "blocked" => prop_assert_eq!(got, EngineError::Blocked),
                "no_results" => prop_assert_eq!(got, EngineError::NoResults),
                "timeout" => prop_assert_eq!(got, EngineError::Timeout),
                "parse" => prop_assert_eq!(
                    got,
                    EngineError::Parse(if colon {
                        detail.trim().to_string()
                    } else {
                        String::new()
                    })
                ),
                _ => prop_assert_eq!(got, EngineError::Transport(err.clone())),
            }
        }

        /// The response decoder never panics on arbitrary lines.
        #[test]
        fn decode_never_panics(line in ".*") {
            let _ = engine().decode(&line);
        }

        /// `v` outside the negotiated range is a Transport error checked
        /// before any `error`-field mapping.
        #[test]
        fn decode_rejects_out_of_range_version(
            v in any::<u8>()
                .prop_filter("out of range", |v| {
                    !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(v)
                }),
            error in prop::option::of(".*"),
        ) {
            let line = json!({"v": v, "error": error, "results": []}).to_string();
            prop_assert!(matches!(
                engine().decode(&line),
                Err(EngineError::Transport(_))
            ));
        }

        /// A non-empty `error` maps through the protocol table verbatim.
        #[test]
        fn decode_maps_protocol_error(
            v in MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION,
            err in ".+",
        ) {
            let line = json!({"v": v, "error": err, "results": []}).to_string();
            prop_assert_eq!(engine().decode(&line), Err(map_protocol_error(&err)));
        }

        /// Rows with unparseable urls are dropped; every emitted result
        /// keeps its row's fields verbatim and in order.
        #[test]
        fn decode_drops_bad_result_urls(
            rows in prop::collection::vec((".*", ".*", ".*"), 0..8),
        ) {
            let line = json!({
                "v": PROTOCOL_VERSION,
                "results": rows
                    .iter()
                    .map(|(t, u, s)| json!({"title": t, "url": u, "snippet": s}))
                    .collect::<Vec<_>>(),
            })
            .to_string();
            let out = engine().decode(&line).unwrap();
            let kept: Vec<_> = rows
                .iter()
                .filter(|(_, u, _)| Url::parse(u).is_ok())
                .collect();
            prop_assert_eq!(out.len(), kept.len());
            for (r, (t, u, s)) in out.iter().zip(kept) {
                prop_assert_eq!(&r.url, &Url::parse(u).unwrap());
                prop_assert_eq!(r.title.as_str(), t.as_str());
                prop_assert_eq!(r.snippet.as_str(), s.as_str());
            }
        }
    }
}
