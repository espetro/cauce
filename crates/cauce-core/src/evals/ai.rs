//! AI answer evals (W4-04, `.agents/plans/v3/wave-4-ai-mode.md`): offline
//! scoring of the grounded-answer loop over recorded provider transcripts
//! plus replay cassettes — the `cauce eval ai` machinery.
//!
//! Case files are JSONL at `evals/ai/*.jsonl`, one object per line:
//!
//! ```json
//! {"query": "current weather in Tokyo right now",
//!  "transcript": "tokyo-weather",
//!  "must_cite_domains": ["jma.go.jp"],
//!  "must_contain": ["22"],
//!  "must_not_contain": ["related_questions"],
//!  "tags": ["smoke"]}
//! ```
//!
//! `transcript` names `evals/ai/transcripts/<stem>.json`: one recorded
//! provider turn per `chat_stream` call, replayed in order by
//! [`TranscriptProvider`]. `cauce eval ai --record` regenerates those
//! files from the live `[ai]` provider via [`RecordingProvider`].
//!
//! Protocol variants (W4-05): `<stem>.json` is the default (OpenAI)
//! transcript; a `<stem>.<proto>.json` file alongside it (e.g.
//! `tokyo-weather.anthropic.json`) adds a variant run of the same case
//! — one outcome per existing variant, marked by
//! [`AiCaseOutcome::protocol`]. `--record` writes `<stem>.json` for
//! `protocol = "openai"` and `<stem>.<proto>.json` otherwise.
//!
//! Scoring is the mean of three per-case rates: cited-domain recall
//! (`must_cite_domains` against the `sources` frame hosts),
//! `must_contain` hits and `must_not_contain` cleanliness on
//! `done.answer`. The ungrounded rate — `done.ungrounded` cases over the
//! run — is a reported metric; whether it gates is `ungrounded_max`'s
//! call in `evals/thresholds.toml`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::ai::{
    AiCallCtx, AiError, AiStreamEvent, ChatCompletion, ChatProvider, ChatRequest, Usage,
};
use crate::evals::{EvalError, domain_matches};
use crate::normalize_url;
use crate::store::AnswerSource;

/// One case line of an `evals/ai/*.jsonl` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiEvalCase {
    /// The user question fed to `AnswerLoop::stream_answer`.
    pub query: String,
    /// Stem of `evals/ai/transcripts/<stem>.json` — the provider turns
    /// replayed for this case, and the file `--record` overwrites.
    pub transcript: String,
    /// Domains expected among the cited source hosts (recall = fraction
    /// matched; an empty list scores 1.0).
    #[serde(default)]
    pub must_cite_domains: Vec<String>,
    /// Substrings `done.answer` must contain.
    #[serde(default)]
    pub must_contain: Vec<String>,
    /// Substrings `done.answer` must not contain — where a metadata-tail
    /// leak (e.g. a broken tail parser leaving `related_questions` in the
    /// body) shows up.
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    /// Selector tags; `cauce eval ai --tag smoke` runs the CI fast gate.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Parse every case in a JSONL file, mirroring
/// [`crate::evals::load_cases`]: blank lines skipped, malformed or
/// semantically empty lines are errors.
pub fn load_cases(path: &Path) -> Result<Vec<AiEvalCase>, EvalError> {
    let text = std::fs::read_to_string(path)?;
    let mut cases = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let line_no = idx + 1;
        let case: AiEvalCase =
            serde_json::from_str(line).map_err(|source| EvalError::CaseJson {
                path: path.to_path_buf(),
                line: line_no,
                source,
            })?;
        if case.query.trim().is_empty() {
            return Err(EvalError::CaseShape {
                path: path.to_path_buf(),
                line: line_no,
                reason: "query is empty".to_string(),
            });
        }
        if case.transcript.trim().is_empty() {
            return Err(EvalError::CaseShape {
                path: path.to_path_buf(),
                line: line_no,
                reason: "transcript is empty".to_string(),
            });
        }
        cases.push(case);
    }
    Ok(cases)
}

/// The cases carrying at least one of `tags`; empty `tags` keeps all.
pub fn tagged(cases: Vec<AiEvalCase>, tags: &[String]) -> Vec<AiEvalCase> {
    if tags.is_empty() {
        return cases;
    }
    cases
        .into_iter()
        .filter(|c| c.tags.iter().any(|t| tags.contains(t)))
        .collect()
}

/// A recorded provider conversation: one [`TranscriptTurn`] per
/// `chat_stream` call, in order. Assembled turns (deltas + completion),
/// not raw SSE — this is exactly what `--record` can capture and what
/// the loop consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// Model id reported as `provider.model()` on replay.
    pub model: String,
    /// Provider base URL the recording was taken against (provenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Wire protocol the recording was taken with (`"openai"` when
    /// absent — every pre-W4-05 transcript). Provenance only: replay is
    /// protocol-agnostic, the filename carries the variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// When `--record` wrote the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<DateTime<Utc>>,
    pub turns: Vec<TranscriptTurn>,
}

/// One recorded `chat_stream` turn: either a `completion` (with the
/// `deltas` that preceded it) or an `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptTurn {
    /// Content deltas replayed before the completion; when empty, the
    /// whole `completion.content` replays as one delta.
    #[serde(default)]
    pub deltas: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<TranscriptCompletion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<TranscriptError>,
}

impl TranscriptTurn {
    /// The `AiStreamEvent`s this turn replays, in order.
    fn events(&self) -> Result<Vec<AiStreamEvent>, AiError> {
        if let Some(e) = &self.error {
            return Ok(vec![AiStreamEvent::Error(e.to_ai_error())]);
        }
        let Some(c) = &self.completion else {
            return Err(AiError::Parse(
                "transcript turn carries neither completion nor error".to_string(),
            ));
        };
        let mut events: Vec<AiStreamEvent> = self
            .deltas
            .iter()
            .cloned()
            .map(AiStreamEvent::Delta)
            .collect();
        if events.is_empty() && !c.content.is_empty() {
            events.push(AiStreamEvent::Delta(c.content.clone()));
        }
        events.push(AiStreamEvent::Done(Box::new(c.to_completion())));
        Ok(events)
    }
}

/// The transcript-owned copy of [`ChatCompletion`] — `ToolCall`'s serde
/// is the provider wire shape (`{"type":"function","function":{...}}`),
/// so transcripts store the flat `{id, name, arguments}` form instead.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TranscriptCompletion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<TranscriptToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl TranscriptCompletion {
    fn to_completion(&self) -> ChatCompletion {
        ChatCompletion {
            id: self.id.clone(),
            model: self.model.clone(),
            content: self.content.clone(),
            tool_calls: self
                .tool_calls
                .iter()
                .map(|t| crate::ai::ToolCall {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    arguments: t.arguments.clone(),
                })
                .collect(),
            finish_reason: self.finish_reason.clone(),
            usage: self.usage,
        }
    }
}

impl From<&ChatCompletion> for TranscriptCompletion {
    fn from(c: &ChatCompletion) -> Self {
        Self {
            id: c.id.clone(),
            model: c.model.clone(),
            content: c.content.clone(),
            tool_calls: c
                .tool_calls
                .iter()
                .map(|t| TranscriptToolCall {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    arguments: t.arguments.clone(),
                })
                .collect(),
            finish_reason: c.finish_reason.clone(),
            usage: c.usage,
        }
    }
}

/// Flat `{id, name, arguments}` tool call — `arguments` verbatim JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// A recorded provider failure; `kind` is the [`AiError::outcome_label`]
/// tag (`"auth"`, `"rate_limited"`, `"context_length"`, `"provider"`,
/// `"timeout"`, `"transport"`, `"parse"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptError {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_s: Option<u64>,
}

impl TranscriptError {
    fn to_ai_error(&self) -> AiError {
        match self.kind.as_str() {
            "auth" => AiError::Auth(self.message.clone()),
            "rate_limited" => AiError::RateLimited {
                retry_after_s: self.retry_after_s,
            },
            "context_length" => AiError::ContextLength(self.message.clone()),
            "provider" => AiError::Provider {
                status: self.status.unwrap_or(500),
                message: self.message.clone(),
            },
            "timeout" => AiError::Timeout,
            "transport" => AiError::Transport(self.message.clone()),
            _ => AiError::Parse(self.message.clone()),
        }
    }
}

impl From<&AiError> for TranscriptError {
    fn from(e: &AiError) -> Self {
        Self {
            kind: e.outcome_label().to_string(),
            message: e.to_string(),
            status: match e {
                AiError::Provider { status, .. } => Some(*status),
                _ => None,
            },
            retry_after_s: match e {
                AiError::RateLimited { retry_after_s } => *retry_after_s,
                _ => None,
            },
        }
    }
}

/// Load `<dir>/<stem>.json` as a [`Transcript`].
pub fn load_transcript(dir: &Path, stem: &str) -> Result<Transcript, EvalError> {
    let path = dir.join(format!("{stem}.json"));
    let text = std::fs::read_to_string(&path)?;
    serde_json::from_str(&text).map_err(|source| EvalError::TranscriptJson {
        path: path.clone(),
        source,
    })
}

/// One `(file stem, protocol)` pair per transcript variant of `stem`
/// on disk: `("<stem>", "openai")` first — the default file, present
/// or not (a missing transcript is a scored skip, never a silent
/// drop) — then each `<stem>.<proto>.json` in the directory, sorted.
pub fn transcript_variants(dir: &Path, stem: &str) -> Vec<(String, String)> {
    let mut variants = vec![(stem.to_string(), "openai".to_string())];
    let prefix = format!("{stem}.");
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut extra: Vec<(String, String)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_str()?;
                let file_stem = name.strip_suffix(".json")?;
                let proto = file_stem.strip_prefix(&prefix)?;
                if proto.is_empty() {
                    return None;
                }
                Some((file_stem.to_string(), proto.to_string()))
            })
            .collect();
        extra.sort();
        variants.extend(extra);
    }
    variants
}

/// The stem `--record` writes for `proto`: `<stem>` for `"openai"`
/// (backward compatible), `<stem>.<proto>` for any other protocol.
pub fn record_stem(stem: &str, proto: &str) -> String {
    match proto {
        "" | "openai" => stem.to_string(),
        other => format!("{stem}.{other}"),
    }
}

/// Write `transcript` to `<dir>/<stem>.json` (pretty JSON, trailing
/// newline); the `--record` side of [`load_transcript`].
pub fn save_transcript(
    dir: &Path,
    stem: &str,
    transcript: &Transcript,
) -> Result<PathBuf, EvalError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{stem}.json"));
    let json =
        serde_json::to_string_pretty(transcript).map_err(|source| EvalError::TranscriptJson {
            path: path.clone(),
            source,
        })?;
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(path)
}

/// A [`ChatProvider`] that replays one [`Transcript`] turn per
/// `chat_stream` call, in order. A call past the transcript's end is a
/// `Parse` error — a loop asking for more turns than were recorded is a
/// case bug, surfaced rather than papered over.
pub struct TranscriptProvider {
    model: String,
    total: usize,
    turns: Mutex<VecDeque<TranscriptTurn>>,
}

impl TranscriptProvider {
    pub fn new(transcript: Transcript) -> Self {
        Self {
            model: transcript.model,
            total: transcript.turns.len(),
            turns: Mutex::new(transcript.turns.into()),
        }
    }

    /// Turns consumed so far (tests).
    pub fn calls(&self) -> usize {
        self.total - self.turns.lock().expect("turns mutex").len()
    }
}

impl ChatProvider for TranscriptProvider {
    fn model(&self) -> &str {
        &self.model
    }

    fn chat_stream(
        &self,
        _req: &ChatRequest,
        _budget: Duration,
        _ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError> {
        let turn = self
            .turns
            .lock()
            .expect("turns mutex")
            .pop_front()
            .ok_or_else(|| AiError::Parse("eval transcript exhausted".to_string()))?;
        let (tx, rx) = mpsc::unbounded_channel();
        match turn.events() {
            Ok(events) => {
                for e in events {
                    let _ = tx.send(e);
                }
            }
            Err(e) => {
                let _ = tx.send(AiStreamEvent::Error(e));
            }
        }
        Ok(rx)
    }
}

/// Wraps a live [`ChatProvider`], forwarding events unchanged while
/// capturing each turn into a [`Transcript`] — the `--record` path.
/// Recording is passive: deltas before the terminal event land in the
/// turn's `deltas`, the terminal event becomes `completion` or `error`.
pub struct RecordingProvider {
    inner: Arc<dyn ChatProvider>,
    model: String,
    provider: Option<String>,
    /// Wire protocol (`"openai"`/`"anthropic"`) the recording came
    /// from — provenance stamped on the transcript.
    protocol: Option<String>,
    turns: Arc<Mutex<Vec<TranscriptTurn>>>,
    pending: Arc<Mutex<Vec<String>>>,
}

impl RecordingProvider {
    pub fn new(inner: Arc<dyn ChatProvider>) -> Self {
        Self {
            model: inner.model().to_string(),
            provider: None,
            protocol: None,
            inner,
            turns: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record the provider base URL (or a label) into the transcript's
    /// provenance field.
    pub fn with_provider_label(mut self, label: impl Into<String>) -> Self {
        self.provider = Some(label.into());
        self
    }

    /// Record the wire protocol into the transcript's `protocol`
    /// provenance field (the `[ai].protocol` of the recording run).
    pub fn with_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }

    /// The recorded turns so far as a [`Transcript`] (consumes them).
    pub fn transcript(&self) -> Transcript {
        Transcript {
            model: self.model.clone(),
            provider: self.provider.clone(),
            protocol: self.protocol.clone(),
            recorded_at: Some(Utc::now()),
            turns: std::mem::take(&mut *self.turns.lock().expect("turns mutex")),
        }
    }
}

impl ChatProvider for RecordingProvider {
    fn model(&self) -> &str {
        &self.model
    }

    fn chat_stream(
        &self,
        req: &ChatRequest,
        budget: Duration,
        ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError> {
        let mut inner_rx = self.inner.chat_stream(req, budget, ctx)?;
        let (tx, rx) = mpsc::unbounded_channel();
        let turns = Arc::clone(&self.turns);
        let pending = Arc::clone(&self.pending);
        tokio::spawn(async move {
            while let Some(ev) = inner_rx.recv().await {
                match &ev {
                    AiStreamEvent::Delta(text) => {
                        pending.lock().expect("pending mutex").push(text.clone());
                    }
                    AiStreamEvent::Done(completion) => {
                        let deltas = std::mem::take(&mut *pending.lock().expect("pending mutex"));
                        turns.lock().expect("turns mutex").push(TranscriptTurn {
                            deltas,
                            completion: Some(TranscriptCompletion::from(&**completion)),
                            error: None,
                        });
                    }
                    AiStreamEvent::Error(e) => {
                        let deltas = std::mem::take(&mut *pending.lock().expect("pending mutex"));
                        turns.lock().expect("turns mutex").push(TranscriptTurn {
                            deltas,
                            completion: None,
                            error: Some(TranscriptError::from(e)),
                        });
                    }
                }
                if tx.send(ev).is_err() {
                    return;
                }
            }
        });
        Ok(rx)
    }
}

/// Per-case outcome — the auditable detail behind the aggregate score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiCaseOutcome {
    pub query: String,
    /// Transcript file stem the case ran on (`<stem>` or
    /// `<stem>.<proto>`).
    pub transcript: String,
    /// Wire protocol of the replayed transcript (`"openai"` for the
    /// default `<stem>.json`).
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Mean of `cited_recall`, `contain_rate`, `clean_rate`.
    pub score: f64,
    /// Every check passed (all three rates are 1.0).
    pub ok: bool,
    /// Fraction of `must_cite_domains` found among source hosts.
    pub cited_recall: f64,
    /// Fraction of `must_contain` found in the answer.
    pub contain_rate: f64,
    /// 1.0 unless `must_not_contain` strings leaked into the answer.
    pub clean_rate: f64,
    /// Hosts of the cited (normalized) source URLs.
    pub cited_hosts: Vec<String>,
    /// `must_cite_domains` entries no source host matched.
    pub missing_domains: Vec<String>,
    /// `must_contain` entries absent from the answer.
    pub missing_strings: Vec<String>,
    /// `must_not_contain` entries found in the answer.
    pub leaked_strings: Vec<String>,
    /// `done.ungrounded` — the answer drew on no sources.
    pub ungrounded: bool,
    /// Why the case could not score (missing transcript, no done frame,
    /// cassette misses — never a silent 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl AiCaseOutcome {
    /// A 0-scored outcome for a case that could not run — missing or
    /// corrupt transcript, store/temp-dir failure. Never a silent drop:
    /// a dropped case would inflate the mean.
    pub fn skipped(case: &AiEvalCase, note: impl Into<String>) -> Self {
        Self {
            query: case.query.clone(),
            transcript: case.transcript.clone(),
            score: 0.0,
            ok: false,
            cited_recall: 0.0,
            contain_rate: 0.0,
            clean_rate: 0.0,
            cited_hosts: vec![],
            missing_domains: case.must_cite_domains.clone(),
            missing_strings: case.must_contain.clone(),
            leaked_strings: vec![],
            ungrounded: true,
            note: Some(note.into()),
            protocol: default_protocol(),
        }
    }
}

/// `AiCaseOutcome::protocol` when absent — `"openai"` is the
/// pre-W4-05 convention (`<stem>.json` files).
fn default_protocol() -> String {
    "openai".to_string()
}

fn rate(hits: usize, expected: usize) -> f64 {
    if expected == 0 {
        1.0
    } else {
        hits as f64 / expected as f64
    }
}

/// Score one case's drained `stream_answer` frames against its checks.
/// The `sources` frame supplies the cited hosts; the `done` frame the
/// answer text. No `done` (provider error, iterations exhausted, missing
/// transcript upstream) is a 0-scored outcome with the terminal error as
/// the note — like `eval engines`' missing-cassette miss, never silent.
pub fn score_frames(case: &AiEvalCase, frames: &[crate::ai::AnswerFrame]) -> AiCaseOutcome {
    use crate::ai::AnswerFrame;

    let sources: Vec<AnswerSource> = frames
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Sources { sources } => Some(sources.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let cited_hosts: Vec<String> = sources
        .iter()
        .filter_map(|s| normalize_url(&s.url).host_str().map(str::to_string))
        .collect();

    let done = frames
        .iter()
        .find(|f| matches!(f, AnswerFrame::Done { .. }));
    let Some(AnswerFrame::Done {
        answer, ungrounded, ..
    }) = done
    else {
        let message = frames.iter().find_map(|f| match f {
            AnswerFrame::Error { message, .. } => Some(message.clone()),
            _ => None,
        });
        return AiCaseOutcome::skipped(
            case,
            message.unwrap_or_else(|| "stream ended without a done frame".to_string()),
        );
    };

    let missing_domains: Vec<String> = case
        .must_cite_domains
        .iter()
        .filter(|d| !cited_hosts.iter().any(|h| domain_matches(h, d)))
        .cloned()
        .collect();
    let missing_strings: Vec<String> = case
        .must_contain
        .iter()
        .filter(|s| !answer.contains(s.as_str()))
        .cloned()
        .collect();
    let leaked_strings: Vec<String> = case
        .must_not_contain
        .iter()
        .filter(|s| answer.contains(s.as_str()))
        .cloned()
        .collect();

    let cited_recall = rate(
        case.must_cite_domains.len() - missing_domains.len(),
        case.must_cite_domains.len(),
    );
    let contain_rate = rate(
        case.must_contain.len() - missing_strings.len(),
        case.must_contain.len(),
    );
    let clean_rate = rate(
        case.must_not_contain.len() - leaked_strings.len(),
        case.must_not_contain.len(),
    );
    let score = (cited_recall + contain_rate + clean_rate) / 3.0;
    AiCaseOutcome {
        query: case.query.clone(),
        transcript: case.transcript.clone(),
        score,
        ok: cited_recall == 1.0 && contain_rate == 1.0 && clean_rate == 1.0,
        cited_recall,
        contain_rate,
        clean_rate,
        cited_hosts,
        missing_domains,
        missing_strings,
        leaked_strings,
        ungrounded: *ungrounded,
        note: None,
        protocol: default_protocol(),
    }
}

/// `[ai]` section of `evals/thresholds.toml` (W4-04): the baseline the
/// committed transcripts score today and the slack the gate tolerates —
/// pass at `score >= baseline - tolerance` (plus `ungrounded_max` when
/// set). A missing `[ai]` section fails `cauce eval ai`, not parses to
/// zero — a gate with no baseline would pass anything.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AiThresholds {
    /// Score the committed case set achieves today.
    pub baseline: f64,
    /// How far below baseline the score may drift before `--gate` fails.
    #[serde(default)]
    pub tolerance: f64,
    /// Ceiling on the ungrounded-case rate; absent = report only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ungrounded_max: Option<f64>,
}

/// The CI smoke rule: `score >= baseline - tolerance` and, when
/// `ungrounded_max` is set, `ungrounded_rate <= ungrounded_max`.
pub fn gate_ok(score: f64, ungrounded_rate: f64, t: &AiThresholds) -> bool {
    score >= t.baseline - t.tolerance && t.ungrounded_max.is_none_or(|max| ungrounded_rate <= max)
}

/// The `evals/results/<date>-ai.json` artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiEvalReport {
    /// Always `"ai"`.
    pub kind: String,
    /// `YYYY-MM-DD` — the filename stem.
    pub date: String,
    pub generated_at: DateTime<Utc>,
    /// Mean per-case score over the run set.
    pub score: f64,
    pub cases: u32,
    /// Cases whose checks all passed.
    pub ok_cases: u32,
    /// `done.ungrounded` cases over the run — a metric; the thresholds
    /// decide whether it gates.
    pub ungrounded_rate: f64,
    /// Queries of the ungrounded cases.
    pub ungrounded_cases: Vec<String>,
    /// `score >= baseline - tolerance` and `ungrounded_rate` under the
    /// optional ceiling — what `--gate` exits on.
    pub gate_ok: bool,
    /// Tag filter the run used (empty = whole set).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Per-case detail, in case-file order (deterministic — not
    /// completion order).
    pub outcomes: Vec<AiCaseOutcome>,
}

impl AiEvalReport {
    /// Stamp `date`/`generated_at` at now (UTC) and assemble the report.
    pub fn new(thresholds: &AiThresholds, tags: Vec<String>, outcomes: Vec<AiCaseOutcome>) -> Self {
        let now = Utc::now();
        let cases = outcomes.len() as u32;
        let score = if cases == 0 {
            0.0
        } else {
            outcomes.iter().map(|o| o.score).sum::<f64>() / cases as f64
        };
        let ok_cases = outcomes.iter().filter(|o| o.ok).count() as u32;
        let ungrounded_cases: Vec<String> = outcomes
            .iter()
            .filter(|o| o.ungrounded)
            .map(|o| o.query.clone())
            .collect();
        let ungrounded_rate = rate(ungrounded_cases.len(), cases as usize);
        Self {
            kind: "ai".to_string(),
            date: now.format("%Y-%m-%d").to_string(),
            generated_at: now,
            score,
            cases,
            ok_cases,
            ungrounded_rate,
            ungrounded_cases,
            gate_ok: gate_ok(score, ungrounded_rate, thresholds),
            tags,
            outcomes,
        }
    }
}

/// Write `report` to `<dir>/<report.date>-ai.json`; returns the path.
pub fn write_report(dir: &Path, report: &AiEvalReport) -> Result<PathBuf, EvalError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}-ai.json", report.date));
    let json = serde_json::to_string_pretty(report).expect("AiEvalReport serializes");
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::AnswerFrame;
    use uuid::Uuid;

    fn case() -> AiEvalCase {
        AiEvalCase {
            query: "q".to_string(),
            transcript: "t".to_string(),
            must_cite_domains: vec!["jma.go.jp".to_string()],
            must_contain: vec!["22".to_string()],
            must_not_contain: vec!["related_questions".to_string()],
            tags: vec!["smoke".to_string()],
        }
    }

    fn done(answer: &str, ungrounded: bool) -> AnswerFrame {
        AnswerFrame::Done {
            answer: answer.to_string(),
            confidence: 8,
            model: "m".to_string(),
            related_questions: vec![],
            cached: false,
            request_id: Uuid::nil(),
            ungrounded,
        }
    }

    fn sources(hosts: &[&str]) -> AnswerFrame {
        AnswerFrame::Sources {
            sources: hosts
                .iter()
                .map(|h| AnswerSource {
                    url: url::Url::parse(&format!("https://{h}/p")).unwrap(),
                    title: "t".to_string(),
                    snippet: "s".to_string(),
                    engine: crate::EngineId::from("replay"),
                })
                .collect(),
        }
    }

    #[test]
    fn score_frames_full_pass() {
        let frames = vec![
            sources(&["jma.go.jp", "timeanddate.com"]),
            done("Tokyo is **22°C** [1]", false),
        ];
        let o = score_frames(&case(), &frames);
        assert_eq!(o.score, 1.0);
        assert!(o.ok);
        assert!(!o.ungrounded);
    }

    #[test]
    fn score_frames_metadata_tail_leak_fails_clean_rate() {
        // A broken tail parser leaves `{"confidence":..,"related_questions":..}`
        // in the answer — exactly what must_not_contain catches.
        let frames = vec![
            sources(&["jma.go.jp"]),
            done(
                "Tokyo is 22\n{\"confidence\": 8, \"related_questions\": []}",
                false,
            ),
        ];
        let o = score_frames(&case(), &frames);
        assert_eq!(o.leaked_strings, vec!["related_questions".to_string()]);
        assert!((o.score - 2.0 / 3.0).abs() < 1e-9);
        assert!(!o.ok);
    }

    #[test]
    fn score_frames_missing_done_scores_zero_with_note() {
        let frames = vec![AnswerFrame::Error {
            message: "provider rate limited".to_string(),
            retry_after_s: Some(3),
        }];
        let o = score_frames(&case(), &frames);
        assert_eq!(o.score, 0.0);
        assert_eq!(o.note.as_deref(), Some("provider rate limited"));
    }

    #[test]
    fn score_frames_missing_domain_partial_recall() {
        let mut c = case();
        c.must_cite_domains.push("example.org".to_string());
        let frames = vec![sources(&["jma.go.jp"]), done("22", false)];
        let o = score_frames(&c, &frames);
        assert_eq!(o.cited_recall, 0.5);
        assert_eq!(o.missing_domains, vec!["example.org".to_string()]);
    }

    #[test]
    fn transcript_provider_replays_turns_then_fails() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let provider = TranscriptProvider::new(Transcript {
                model: "m".to_string(),
                provider: None,
                protocol: None,
                recorded_at: None,
                turns: vec![
                    TranscriptTurn {
                        deltas: vec![],
                        completion: Some(TranscriptCompletion {
                            id: None,
                            model: None,
                            content: String::new(),
                            tool_calls: vec![TranscriptToolCall {
                                id: "c1".to_string(),
                                name: "search_web".to_string(),
                                arguments: "{\"query\": \"x\"}".to_string(),
                            }],
                            finish_reason: Some("tool_calls".to_string()),
                            usage: None,
                        }),
                        error: None,
                    },
                    TranscriptTurn {
                        deltas: vec![],
                        completion: None,
                        error: Some(TranscriptError {
                            kind: "rate_limited".to_string(),
                            message: "slow down".to_string(),
                            status: None,
                            retry_after_s: Some(7),
                        }),
                    },
                ],
            });
            let req = ChatRequest::default();
            let ctx = AiCallCtx::default();
            let mut rx = provider
                .chat_stream(&req, Duration::from_secs(1), ctx.clone())
                .unwrap();
            let done = loop {
                match rx.recv().await {
                    Some(AiStreamEvent::Done(c)) => break c,
                    Some(_) => {}
                    None => panic!("stream closed without done"),
                }
            };
            assert_eq!(done.tool_calls[0].name, "search_web");

            let mut rx = provider
                .chat_stream(&req, Duration::from_secs(1), ctx.clone())
                .unwrap();
            match rx.recv().await {
                Some(AiStreamEvent::Error(AiError::RateLimited { retry_after_s })) => {
                    assert_eq!(retry_after_s, Some(7))
                }
                other => panic!("expected rate-limited error, got {other:?}"),
            }

            assert!(
                provider
                    .chat_stream(&req, Duration::from_secs(1), ctx)
                    .is_err()
            );
        });
    }

    #[test]
    fn gate_math() {
        let t = AiThresholds {
            baseline: 1.0,
            tolerance: 0.2,
            ungrounded_max: Some(0.5),
        };
        assert!(gate_ok(1.0, 0.0, &t));
        assert!(gate_ok(0.8, 0.4, &t));
        assert!(!gate_ok(0.79, 0.0, &t));
        assert!(!gate_ok(1.0, 0.6, &t));
    }

    #[test]
    fn tagged_filters_by_tag() {
        let mut other = case();
        other.tags = vec!["edge".to_string()];
        let out = tagged(vec![case(), other], &["smoke".to_string()]);
        assert_eq!(out.len(), 1);
        assert_eq!(tagged(vec![case()], &[]).len(), 1);
    }

    #[test]
    fn transcript_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let t = Transcript {
            model: "m".to_string(),
            provider: Some("https://example.test/v1".to_string()),
            protocol: None,
            recorded_at: Some(Utc::now()),
            turns: vec![TranscriptTurn {
                deltas: vec!["a".to_string()],
                completion: Some(TranscriptCompletion::default()),
                error: None,
            }],
        };
        save_transcript(dir.path(), "t1", &t).unwrap();
        let loaded = load_transcript(dir.path(), "t1").unwrap();
        assert_eq!(loaded.model, "m");
        assert_eq!(loaded.turns.len(), 1);
    }
}
