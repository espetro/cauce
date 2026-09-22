//! `exec` engine runtime: one warm child process per engine, JSON lines over
//! stdin/stdout (parent plan section 4.3, protocol `v: 1`).
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

use oxe_core::{
    Engine, EngineError, EngineId, EnginePhase, Metrics, SearchRequest, SearchResult, Tier,
};

/// Version of the exec wire protocol implemented here (parent plan 4.3).
pub const PROTOCOL_VERSION: u8 = 1;

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
}

impl ExecSpec {
    /// The day-1 bridge from the wave-0 plan
    /// (`[[engines]] id="ddgs" kind="exec" command="python3"
    /// args=["sdk/python/oxe_engine_sdk/ddgs_auto.py"]`, tier 2, page_size 10).
    ///
    /// `cwd` is the directory the relative script arg resolves against,
    /// typically the repo or install root.
    pub fn ddgs(cwd: PathBuf) -> Self {
        Self {
            id: EngineId::new("ddgs"),
            command: "python3".to_string(),
            args: vec!["sdk/python/oxe_engine_sdk/ddgs_auto.py".to_string()],
            env: vec![],
            cwd: Some(cwd),
            page_size: 10,
            tier: Tier::T2,
        }
    }
}

/// Outbound request line (`->` on the child's stdin), protocol v1.
///
/// Field names are the settled wire contract: `query`, `page`, `lang`,
/// `timeout_ms`.
#[derive(Debug, Clone, Serialize)]
pub struct ExecRequest {
    pub v: u8,
    pub query: String,
    pub page: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    pub timeout_ms: u64,
}

impl ExecRequest {
    fn new(req: &SearchRequest, budget: Duration) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            query: req.q.clone(),
            page: req.page,
            lang: req.lang.clone(),
            timeout_ms: budget.as_millis().clamp(1, u64::MAX as u128) as u64,
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

/// Inbound response line (`<-` on the child's stdout), protocol v1.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecResponse {
    pub v: u8,
    #[serde(default)]
    pub results: Vec<ExecResultRow>,
    #[serde(default)]
    pub error: Option<String>,
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

/// A warm `exec` engine: owns one child process and speaks protocol v1 to it.
///
/// The child is spawned eagerly when `new` runs inside a tokio runtime (the
/// `serve`/`mcp` factory path, tests); a failed spawn degrades to the lazy
/// path, so `new` is safe to call outside a runtime and never fails on a
/// missing `command`.
pub struct ExecEngine {
    spec: ExecSpec,
    state: Arc<Mutex<State>>,
    /// W1-09 phase timings (`oxe_engine_duration_ms{phase}`): `http` is the
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
        if resp.v != PROTOCOL_VERSION {
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
        let mut line = serde_json::to_string(&ExecRequest::new(req, budget))
            .map_err(|e| EngineError::Parse(format!("request encode: {e}")))?;
        line.push('\n');

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

        let round_trip = async {
            io.stdin.write_all(line.as_bytes()).await?;
            io.stdin.flush().await?;
            let mut buf = String::new();
            io.stdout.read_line(&mut buf).await?;
            Ok::<String, std::io::Error>(buf)
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
                warn!(engine = %self.spec.id, "exec child io failed ({e}); killing child");
                return Err(EngineError::Transport(format!("child io: {e}")));
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
