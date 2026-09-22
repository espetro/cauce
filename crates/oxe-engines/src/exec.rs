//! `exec` engine runtime: one warm child process per engine, JSON lines over
//! stdin/stdout (parent plan section 4.3, protocol `v: 1`).
//!
//! One request is in flight per process; concurrent `search` calls serialize on
//! a mutex. A child that crashes or overruns the request budget is killed and
//! respawned lazily on the next call. Child stderr is forwarded to `tracing`
//! at `warn`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, warn};

use oxe_core::{Engine, EngineError, EngineId, SearchRequest, SearchResult, Tier};

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

/// A warm `exec` engine: owns one child process and speaks protocol v1 to it.
///
/// The child is spawned lazily on the first `search` call, so `new` is safe to
/// call outside a tokio runtime.
pub struct ExecEngine {
    spec: ExecSpec,
    state: Mutex<State>,
}

impl ExecEngine {
    pub fn new(spec: ExecSpec) -> Self {
        Self {
            spec,
            state: Mutex::new(State::default()),
        }
    }

    pub fn spec(&self) -> &ExecSpec {
        &self.spec
    }

    /// Spawn the child. Must be called inside a tokio runtime (the stderr
    /// forwarder is a spawned task).
    fn spawn(&self) -> Result<ChildIo, EngineError> {
        let mut cmd = Command::new(&self.spec.command);
        cmd.args(&self.spec.args)
            .envs(self.spec.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &self.spec.cwd {
            cmd.current_dir(cwd);
        }
        let mut child = cmd.spawn().map_err(|e| {
            EngineError::Transport(format!("spawn `{}` failed: {e}", self.spec.command))
        })?;
        if let Some(stderr) = child.stderr.take() {
            let id = self.spec.id.clone();
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
        debug!(engine = %self.spec.id, "exec child spawned");
        Ok(ChildIo {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Return the live child, respawning first when the previous one exited.
    async fn ensure_child<'a>(&self, state: &'a mut State) -> Result<&'a mut ChildIo, EngineError> {
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
            state.child = Some(self.spawn()?);
        }
        Ok(state.child.as_mut().expect("child present"))
    }

    /// Kill the current child (if any) and forget it; the next call respawns.
    async fn kill_child(&self, state: &mut State) {
        if let Some(mut io) = state.child.take() {
            let _ = io.child.kill().await;
        }
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
        // round trip.
        let mut state = self.state.lock().await;
        let io = self.ensure_child(&mut state).await?;

        let round_trip = async {
            io.stdin.write_all(line.as_bytes()).await?;
            io.stdin.flush().await?;
            let mut buf = String::new();
            io.stdout.read_line(&mut buf).await?;
            Ok::<String, std::io::Error>(buf)
        };

        match timeout(budget, round_trip).await {
            Err(_) => {
                warn!(engine = %self.spec.id, budget_ms = budget.as_millis() as u64,
                    "exec engine deadline hit; killing child");
                self.kill_child(&mut state).await;
                Err(EngineError::Timeout)
            }
            Ok(Err(e)) => {
                warn!(engine = %self.spec.id, "exec child io failed ({e}); killing child");
                self.kill_child(&mut state).await;
                Err(EngineError::Transport(format!("child io: {e}")))
            }
            Ok(Ok(buf)) if buf.is_empty() => {
                warn!(engine = %self.spec.id, "exec child closed stdout (EOF); will respawn");
                state.child = None;
                Err(EngineError::Transport("engine process exited".into()))
            }
            Ok(Ok(buf)) => self.decode(&buf),
        }
    }
}
