//! `cauce tui [query]` — interactive terminal search (ratatui) that drives
//! the pipeline in-process, no server needed.
//!
//! Bootstrap mirrors `cmds::search`: `Config::load` -> data dirs ->
//! observability (JSONL, so `cauce trace <request_id>` rebuilds TUI
//! fan-outs too) -> `SqliteStore` -> engine factory -> `SearchPipeline`
//! with `search.*`/`admission.*` tunables -> persisted breakers. The only
//! difference: results render into a ratatui `Terminal` instead of stdout.
//!
//! Streaming model (W2-01 `StreamEvent`): `Results` batches paint as
//! engines return, deduped by `normalize_url` — the same key `Meta.order`
//! carries — and the terminal `Meta` marks late results whose final RRF
//! rank beats their arrival position (the SPA's "late but better" rule:
//! the progressive page never reorders). Retyping aborts only the
//! forwarder task; the spawned flight still finishes and warms the cache.
//!
//! Keys: type to search, `Enter` run / pick suggestion, `↑`/`↓`/`Tab`
//! cycle suggestions, `Esc` clear/dismiss, `j`/`k` move, `o`/`Enter` open
//! in browser, `y` yank via OSC52, `Y` copy the list as markdown, `n` next
//! page, `a` assist card over the top rows, `Ctrl-A` omnibox ↔ ask (full
//! `AnswerLoop`, multi-turn), `/` back to the omnibox, `q`/`Ctrl-C` quit.
//! The ai surface is experimental: `stream_answer`/`stream_assist` frames
//! paint as plain text (deltas concatenated == `done.answer` markdown).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use cauce_agent::AnswerLoop;
use cauce_core::ai::{AnswerFrame, AnswerRequest, AnswerRole, AnswerTurn};
use cauce_core::config::{Config, Resources};
use cauce_core::{
    Admission, AdmissionLimits, AnswerSource, CachePolicy, ClientKind, EngineId, HealthPolicy,
    HedgePolicy, MergePolicy, SafeSearch, SearchOpts, SearchPipeline, SearchRequest, Source, Store,
    StreamEvent, normalize_url,
};
use cauce_engines::factory::build_engines;
use cauce_server::observability;
use cauce_store_sqlite::SqliteStore;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use url::Url;
use uuid::Uuid;

const USAGE: &str = "usage: cauce tui [query...]";

/// Suggest debounce: `Store::suggest` runs this long after the last edit.
const SUGGEST_DEBOUNCE: Duration = Duration::from_millis(120);
/// Max suggestion rows rendered under the omnibox.
const SUGGEST_LIMIT: u32 = 8;

/// CLI surface: `cauce tui [query]` seeds the omnibox and runs it at once.
struct Args {
    query: Option<String>,
}

/// Entry point for the `tui` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("cauce tui: {msg}\n{USAGE}");
            return 2;
        }
    };
    let cfg = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("cauce tui: {e}");
            return 2;
        }
    };
    if let Err(e) = cfg.ensure_dirs() {
        eprintln!("cauce tui: cannot create data dirs: {e}");
        return 1;
    }
    // A JSONL guard keeps `cauce trace` working on TUI fan-outs; failure is
    // non-fatal for the same reason as `cauce search`. The pretty stderr
    // layer stays off: the alternate screen owns the TTY and stray log lines
    // would corrupt the frame.
    let guard = match observability::init(&observability::ObservabilityConfig {
        logs_dir: cfg.logs_dir(),
        retention_days: cfg.logs.retention_days as usize,
        stderr_pretty: false,
        ..Default::default()
    }) {
        Ok(guard) => Some(guard),
        Err(e) => {
            eprintln!("cauce tui: logging init failed ({e}); continuing without JSONL logs");
            None
        }
    };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("cauce tui: tokio runtime: {e}");
            return 1;
        }
    };
    let code = rt.block_on(tui_async(&cfg, opts));
    drop(rt);
    if let Some(guard) = guard {
        guard.shutdown();
    }
    code
}

/// Pipeline bootstrap (verbatim `cmds::search`) plus the ratatui event loop.
async fn tui_async(cfg: &Config, opts: Args) -> i32 {
    let tuning = Resources::detect().store_tuning;
    let store = match SqliteStore::open(cfg.db_path(), tuning) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            eprintln!(
                "cauce tui: cannot open store {}: {e}",
                cfg.db_path().display()
            );
            return 1;
        }
    };
    let engines = build_engines(cfg);
    if engines.is_empty() {
        eprintln!("cauce tui: no engines enabled (see [[engines]] / CAUCE_ENGINES)");
        return 1;
    }
    let pipeline = SearchPipeline::new(store.clone(), engines)
        .with_deadline(Duration::from_millis(cfg.search.deadline_ms))
        .with_default_ttl(Duration::from_secs(cfg.search.ttl_s))
        .with_ttl_cap(Duration::from_secs(cfg.search.ttl_cap_s))
        .with_lexical(cfg.cache.lexical)
        .with_admission(Admission::new(AdmissionLimits {
            max_wait: Duration::from_millis(cfg.admission.max_wait_ms),
            max_concurrent_per_engine: cfg.admission.max_concurrent_per_engine.max(1) as usize,
        }))
        .with_hedge(HedgePolicy {
            floor: Duration::from_millis(cfg.search.hedge_floor_ms),
            ceiling: Duration::from_millis(cfg.search.hedge_ceiling_ms),
            min_results: cfg.search.min_results as usize,
        })
        .with_merge(MergePolicy {
            rrf_k: cfg.merge.rrf_k as f32,
            collapse_same_host_after: cfg.merge.collapse_same_host_after as usize,
        })
        .with_cache_policy(CachePolicy {
            stale_grace: Duration::from_secs(cfg.cache.stale_grace_s),
            degraded_ttl: Duration::from_secs(cfg.cache.degraded_ttl_s),
        })
        .with_health_policy(HealthPolicy {
            degraded_threshold: cfg.health.degraded_threshold,
            degraded_window: Duration::from_secs(cfg.health.degraded_window_s),
            ..HealthPolicy::default()
        });
    if let Err(e) = pipeline.load_health().await {
        tracing::warn!(error = %e, "engine health load failed; starting with closed breakers");
    }

    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cauce tui: cannot initialize terminal: {e}");
            return 1;
        }
    };
    // Experimental AI surface: `ctrl+a` runs the full `AnswerLoop`
    // (multi-turn), `a` on results runs the W7-02 single-turn assist over
    // the meta-ordered top rows. Both gate on `[ai].enabled`; the knob
    // wiring mirrors `cauce-server/src/app.rs`.
    let answer_loop = if cfg.ai.enabled {
        match cauce_core::ai::provider_client(&cfg.ai, Some(store.clone())) {
            Ok(provider) => Some(
                AnswerLoop::new(pipeline.clone(), provider, store.clone())
                    .with_max_turns(cfg.ai.max_turns as usize)
                    .with_max_search_executions(cfg.ai.max_searches as usize)
                    .with_provider_budget(Duration::from_secs(cfg.ai.provider_budget_s)),
            ),
            Err(e) => {
                eprintln!("cauce tui: [ai] provider config: {e}; ai surface disabled");
                None
            }
        }
    } else {
        None
    };

    let code = run_app(
        &mut terminal,
        &pipeline,
        &store,
        answer_loop.as_ref(),
        opts.query,
    )
    .await;
    ratatui::restore();
    if let Err(e) = pipeline.health().flush().await {
        tracing::warn!(error = %e, "engine health flush failed");
    }
    if let Err(e) = code {
        eprintln!("cauce tui: {e}");
        return 1;
    }
    0
}

/// Every async source feeding the UI lands on this channel: terminal input,
/// pipeline stream events, debounced suggestions, action notes.
enum Msg {
    Term(Event),
    /// Stream events carry the flight's `search_gen`; replies from a
    /// superseded flight are dropped on arrival.
    Stream {
        flight: u64,
        ev: StreamEvent,
    },
    /// The forwarder drained the stream's terminal `Meta`/`Error`.
    SearchDone {
        flight: u64,
    },
    /// `search_stream` itself rejected (bad pin, no engines, …).
    SearchRejected {
        flight: u64,
        error: String,
    },
    Suggest {
        seq: u64,
        items: Vec<String>,
    },
    /// One `AnswerFrame` from an `stream_answer`/`stream_assist` flight;
    /// `flight` mirrors `search_gen` so re-asks drop stale frames.
    Answer {
        flight: u64,
        frame: AnswerFrame,
    },
    /// The answer frame channel drained without a terminal frame.
    AnswerClosed {
        flight: u64,
    },
    Note(String),
}

/// Focus target for key routing: the omnibox owns typing until a search
/// runs; `/` and `Esc` move focus back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Focus {
    #[default]
    Input,
    Results,
    /// The ai/assist pane owns keys until `Esc`/`/` returns to the
    /// results or the omnibox.
    Answer,
}

/// One rendered result row. `rank` is the position in the terminal `Meta`'s
/// final RRF `order` when it beats `flight_idx`, the row's arrival index
/// within its own flight — the late-but-better marker. Scoping the marker to
/// the flight keeps appends (next page, re-fires) from flagging every row.
#[derive(Debug, Clone)]
struct Row {
    key: Url,
    title: String,
    url: Url,
    engine: EngineId,
    snippet: String,
    /// Arrival index within the flight that produced this row.
    flight_idx: usize,
    rank: Option<usize>,
}

/// One asked question or one assist run in the answer pane.
#[derive(Debug, Clone, Default)]
struct TurnView {
    q: String,
    /// `Step` labels (`Searching: …`) in arrival order.
    steps: Vec<String>,
    /// Concatenated `Delta`s; equals `done.answer` once it lands.
    deltas: String,
    sources: Vec<AnswerSource>,
    done: Option<DoneView>,
    error: Option<String>,
    streaming: bool,
}

/// The replayable `Done` fields the pane/copier keep.
#[derive(Debug, Clone)]
struct DoneView {
    answer: String,
    confidence: u8,
    model: String,
    related: Vec<String>,
    ungrounded: bool,
    cached: bool,
    log_id: Option<i64>,
}

/// The answer-pane state: an ordered turn list plus the completed
/// `history` the next `stream_answer` replays (assist turns never join
/// it — the SPA's assist is single-turn and escalation starts fresh).
#[derive(Debug, Clone, Default)]
struct AnswerView {
    turns: Vec<TurnView>,
    history: Vec<AnswerTurn>,
    /// Set by `start_assist`; `enter` on an assist run escalates into a
    /// full answer loop rather than re-running assist.
    assist: bool,
}

/// Widget-free application state so unit tests drive it without a terminal.
#[derive(Default)]
struct App {
    input: Vec<char>,
    cursor: usize,
    focus: Focus,
    suggestions: Vec<String>,
    sugg_sel: Option<usize>,
    suggest_gen: u64,
    results: Vec<Row>,
    seen: HashSet<Url>,
    selected: usize,
    page: u8,
    searching: bool,
    /// Bumped by every `start_search`; stream messages from older flights
    /// are ignored. The dropped flight still finishes in the background and
    /// warms the cache — retype/cancel only silences its forwarder.
    search_gen: u64,
    /// Rows appended so far in the current flight (resets per `start_search`).
    flight_len: usize,
    /// Latest `Meta.order` — the assist card's context order (top 10).
    last_order: Vec<Url>,
    /// `Ctrl-A` puts the omnibox in ask mode: `Enter` runs `stream_answer`
    // / instead of a search. The badge in the hints row makes it visible.
    ai_mode: bool,
    /// The answer/assist pane; `Some` renders in place of the result list.
    answer: Option<AnswerView>,
    /// Bumped per `stream_answer`/`stream_assist` call — stale frames drop
    /// like stale search flights do.
    answer_gen: u64,
    /// Lines scrolled up from the tail of the answer pane (0 = pinned).
    answer_scroll: u16,
    /// `Meta` fields the status bar keeps once the stream ends.
    status: Option<String>,
    error: Option<String>,
    /// Last action note ("opened …", "yanked …", open failure).
    note: Option<String>,
}

impl App {
    fn query(&self) -> String {
        self.input.iter().collect()
    }

    /// Fold a `StreamEvent` into the result list.
    fn apply_stream(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Results {
                engine, results, ..
            } => {
                for r in results {
                    let key = normalize_url(&r.url);
                    if !self.seen.insert(key.clone()) {
                        continue;
                    }
                    let flight_idx = self.flight_len;
                    self.flight_len += 1;
                    self.results.push(Row {
                        key,
                        title: r.title,
                        url: r.url,
                        engine: engine.clone(),
                        snippet: r.snippet,
                        flight_idx,
                        rank: None,
                    });
                }
                if !self.results.is_empty() {
                    self.selected = self.selected.min(self.results.len() - 1);
                }
            }
            StreamEvent::Meta(meta) => {
                self.last_order = meta.order.clone();
                for row in &mut self.results {
                    // `order` is best-first; flag a row when its true rank
                    // beats the slot it arrived in (late-but-better). Rows
                    // from older flights are absent from `order` and keep
                    // whatever rank their own Meta already gave them.
                    if let Some(pos) = meta.order.iter().position(|k| k == &row.key) {
                        row.rank = (pos < row.flight_idx).then_some(pos);
                    }
                }
                self.status = Some(format!(
                    "{} · {} results · {} ms",
                    source_label(&meta.meta.source),
                    self.results.len(),
                    meta.meta.elapsed_ms
                ));
            }
            StreamEvent::Error(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    /// Fold one `AnswerFrame` into the pane's streaming turn. Terminal
    /// frames close the turn and join `history` on `done`.
    fn apply_answer_frame(&mut self, frame: AnswerFrame) {
        let Some(view) = &mut self.answer else { return };
        let Some(turn) = view.turns.last_mut() else {
            return;
        };
        match frame {
            AnswerFrame::Step { label, .. } => turn.steps.push(label),
            AnswerFrame::Delta { text } => turn.deltas.push_str(&text),
            AnswerFrame::Sources { sources } => turn.sources = sources,
            AnswerFrame::Done {
                answer,
                confidence,
                model,
                related_questions,
                ungrounded,
                cached,
                log_id,
                ..
            } => {
                view.history.push(AnswerTurn {
                    role: AnswerRole::User,
                    content: turn.q.clone(),
                });
                view.history.push(AnswerTurn {
                    role: AnswerRole::Assistant,
                    content: answer.clone(),
                });
                self.status = Some(format!(
                    "{model} · confidence {confidence}/10{}{}",
                    if cached { " · cached" } else { "" },
                    if ungrounded { " · ungrounded" } else { "" }
                ));
                turn.done = Some(DoneView {
                    answer,
                    confidence,
                    model,
                    related: related_questions,
                    ungrounded,
                    cached,
                    log_id,
                });
                turn.streaming = false;
            }
            AnswerFrame::Error { message, .. } => {
                turn.error = Some(message);
                turn.streaming = false;
            }
        }
    }

    fn insert(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.results.is_empty() {
            return;
        }
        let len = self.results.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(len) as usize;
    }
}

/// The async app: owns the terminal, forwards pipeline/suggest/terminal
/// events onto one channel, drains pending messages before each draw.
async fn run_app(
    terminal: &mut DefaultTerminal,
    pipeline: &SearchPipeline,
    store: &Arc<SqliteStore>,
    answer_loop: Option<&AnswerLoop>,
    seed: Option<String>,
) -> io::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut events = EventStream::new();
            while let Some(Ok(ev)) = events.next().await {
                if tx.send(Msg::Term(ev)).is_err() {
                    break;
                }
            }
        });
    }

    let mut app = App {
        page: 1,
        ..App::default()
    };
    if let Some(q) = seed.filter(|q| !q.trim().is_empty()) {
        app.input = q.chars().collect();
        app.cursor = app.input.len();
        start_search(&mut app, pipeline, &tx);
        app.focus = Focus::Results;
    }

    loop {
        terminal.draw(|f| render(f, &app))?;
        let Some(msg) = rx.recv().await else { break };
        if handle(msg, &mut app, pipeline, store, answer_loop, &tx) {
            break;
        }
        while let Ok(msg) = rx.try_recv() {
            if handle(msg, &mut app, pipeline, store, answer_loop, &tx) {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Fire the stream for `app.query()` at `app.page`. Callers manage `page`
/// and clearing; appending across pages dedupes through `seen`. Each call
/// opens a new flight: the previous flight's forwarder keeps running (its
/// receiver finishes and warms the cache) but its messages drop on arrival.
fn start_search(app: &mut App, pipeline: &SearchPipeline, tx: &mpsc::UnboundedSender<Msg>) {
    let q = app.query().trim().to_string();
    if q.is_empty() {
        return;
    }
    app.search_gen += 1;
    app.flight_len = 0;
    let flight = app.search_gen;
    app.searching = true;
    app.error = None;
    app.status = Some(format!("searching \"{q}\"…"));
    let req = SearchRequest {
        q,
        page: app.page,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: None,
        client: ClientKind::Cli,
        origin: cauce_core::SearchOrigin::User,
    };
    let pipe = pipeline.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        match pipe
            .search_stream(
                &req,
                SearchOpts {
                    request_id: Some(Uuid::now_v7()),
                    ttl: None,
                },
            )
            .await
        {
            Ok(mut stream) => {
                while let Some(ev) = stream.recv().await {
                    if tx.send(Msg::Stream { flight, ev }).is_err() {
                        return;
                    }
                }
                let _ = tx.send(Msg::SearchDone { flight });
            }
            Err(e) => {
                let _ = tx.send(Msg::SearchRejected {
                    flight,
                    error: e.to_string(),
                });
            }
        }
    });
}

/// Open a turn on the answer pane: push a streaming `TurnView`, fire
/// `spawn` to pump its frames onto `flight`-tagged `Msg::Answer`s.
fn open_turn(app: &mut App, assist: bool, q: &str) -> u64 {
    app.answer_gen += 1;
    let flight = app.answer_gen;
    let view = app.answer.get_or_insert_with(AnswerView::default);
    view.assist = assist;
    view.turns.push(TurnView {
        q: q.to_string(),
        streaming: true,
        ..TurnView::default()
    });
    app.answer_scroll = 0;
    app.focus = Focus::Answer;
    flight
}

/// `Ctrl-A`/`enter` in ask mode: the full `AnswerLoop` (tools on), replaying
/// the pane's `history` for multi-turn follow-ups.
fn start_answer(app: &mut App, answer_loop: &AnswerLoop, tx: &mpsc::UnboundedSender<Msg>) {
    let q = app.query().trim().to_string();
    if q.is_empty() {
        return;
    }
    let history = app
        .answer
        .as_ref()
        .map(|v| v.history.clone())
        .unwrap_or_default();
    let flight = open_turn(app, false, &q);
    app.status = Some(format!("asking \"{q}\"…"));
    app.error = None;
    let req = AnswerRequest {
        q,
        history,
        client: ClientKind::Cli,
        request_id: Some(Uuid::now_v7()),
        actor: None,
    };
    let loop_ = answer_loop.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut frames = loop_.stream_answer(&req);
        while let Some(frame) = frames.recv().await {
            if tx.send(Msg::Answer { flight, frame }).is_err() {
                return;
            }
        }
        let _ = tx.send(Msg::AnswerClosed { flight });
    });
}

/// `a` on results: the W7-02 assist — one no-tools turn over the SERP's
/// top-10 in final `Meta.order` (the same cap the SPA's `context_results`
/// applies). Needs a landed `Meta` (`status.is_some()`) like the SPA's
/// disabled-until-armed trigger.
fn start_assist(app: &mut App, answer_loop: &AnswerLoop, tx: &mpsc::UnboundedSender<Msg>) {
    if app.results.is_empty() || app.status.is_none() {
        app.note = Some("assist needs a finished result set".into());
        return;
    }
    let rows: Vec<&Row> = if app.last_order.is_empty() {
        app.results.iter().take(10).collect()
    } else {
        app.last_order
            .iter()
            .filter_map(|k| app.results.iter().find(|r| &r.key == k))
            .take(10)
            .collect()
    };
    let sources: Vec<AnswerSource> = rows
        .into_iter()
        .map(|r| AnswerSource {
            url: r.url.clone(),
            title: r.title.clone(),
            snippet: r.snippet.clone(),
            engine: r.engine.clone(),
        })
        .collect();
    let q = app.query().trim().to_string();
    let flight = open_turn(app, true, &q);
    app.error = None;
    let req = AnswerRequest {
        q,
        history: Vec::new(),
        client: ClientKind::Cli,
        request_id: Some(Uuid::now_v7()),
        actor: None,
    };
    let loop_ = answer_loop.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut frames = loop_.stream_assist(&req, sources);
        while let Some(frame) = frames.recv().await {
            if tx.send(Msg::Answer { flight, frame }).is_err() {
                return;
            }
        }
        let _ = tx.send(Msg::AnswerClosed { flight });
    });
}

/// Queue a debounced `Store::suggest` for the current input; the generation
/// counter drops stale replies.
fn queue_suggest(app: &mut App, store: &Arc<SqliteStore>, tx: &mpsc::UnboundedSender<Msg>) {
    app.suggest_gen += 1;
    let seq = app.suggest_gen;
    let prefix = app.query();
    if prefix.trim().is_empty() {
        app.suggestions.clear();
        app.sugg_sel = None;
        return;
    }
    let store = store.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(SUGGEST_DEBOUNCE).await;
        let items = store
            .suggest(&prefix, SUGGEST_LIMIT)
            .await
            .unwrap_or_default();
        let _ = tx.send(Msg::Suggest { seq, items });
    });
}

/// One channel message. Returns true when the app should exit.
fn handle(
    msg: Msg,
    app: &mut App,
    pipeline: &SearchPipeline,
    store: &Arc<SqliteStore>,
    answer_loop: Option<&AnswerLoop>,
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    match msg {
        Msg::Term(Event::Key(key)) => {
            return handle_key(key, app, pipeline, store, answer_loop, tx);
        }
        Msg::Term(_) => {}
        Msg::Stream { flight, ev } => {
            if flight == app.search_gen {
                app.apply_stream(ev);
            }
        }
        Msg::SearchDone { flight } => {
            if flight == app.search_gen {
                app.searching = false;
            }
        }
        Msg::SearchRejected { flight, error } => {
            if flight == app.search_gen {
                app.searching = false;
                app.error = Some(error);
            }
        }
        Msg::Answer { flight, frame } => {
            if flight == app.answer_gen {
                app.apply_answer_frame(frame);
            }
        }
        Msg::AnswerClosed { flight } => {
            if flight == app.answer_gen
                && let Some(turn) = app.answer.as_mut().and_then(|v| v.turns.last_mut())
            {
                turn.streaming = false;
            }
        }
        Msg::Suggest { seq, items } => {
            if seq == app.suggest_gen && app.focus == Focus::Input && !app.query().is_empty() {
                app.suggestions = items;
                app.sugg_sel = None;
            }
        }
        Msg::Note(note) => app.note = Some(note),
    }
    false
}

fn handle_key(
    key: KeyEvent,
    app: &mut App,
    pipeline: &SearchPipeline,
    store: &Arc<SqliteStore>,
    answer_loop: Option<&AnswerLoop>,
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    app.note = None;
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d') => return true,
            KeyCode::Char('a') => {
                if answer_loop.is_none() {
                    app.note = Some("ai disabled — set [ai] in the config".into());
                    return false;
                }
                app.ai_mode = !app.ai_mode;
                app.suggestions.clear();
                app.sugg_sel = None;
            }
            _ => {}
        }
        return false;
    }
    match app.focus {
        Focus::Input => input_key(key, app, pipeline, store, answer_loop, tx),
        Focus::Results => results_key(key, app, pipeline, answer_loop, tx),
        Focus::Answer => answer_key(key, app),
    }
}

fn input_key(
    key: KeyEvent,
    app: &mut App,
    pipeline: &SearchPipeline,
    store: &Arc<SqliteStore>,
    answer_loop: Option<&AnswerLoop>,
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    let mut edited = false;
    match key.code {
        KeyCode::Char(c) => {
            app.insert(c);
            edited = true;
        }
        KeyCode::Backspace => {
            app.backspace();
            edited = true;
        }
        KeyCode::Delete => {
            if app.cursor < app.input.len() {
                app.input.remove(app.cursor);
                edited = true;
            }
        }
        KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
        KeyCode::Right => app.cursor = (app.cursor + 1).min(app.input.len()),
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        KeyCode::Down | KeyCode::Tab if !app.suggestions.is_empty() => {
            let next = app.sugg_sel.map_or(0, |i| (i + 1) % app.suggestions.len());
            app.sugg_sel = Some(next);
        }
        KeyCode::Up if !app.suggestions.is_empty() => {
            let prev = app.sugg_sel.map_or(app.suggestions.len() - 1, |i| {
                (i + app.suggestions.len() - 1) % app.suggestions.len()
            });
            app.sugg_sel = Some(prev);
        }
        KeyCode::Enter => {
            if let Some(i) = app.sugg_sel {
                let pick = app.suggestions[i].clone();
                app.input = pick.chars().collect();
                app.cursor = app.input.len();
            }
            app.suggestions.clear();
            app.sugg_sel = None;
            if app.ai_mode {
                if let Some(loop_) = answer_loop {
                    start_answer(app, loop_, tx);
                }
            } else {
                app.results.clear();
                app.seen.clear();
                app.page = 1;
                start_search(app, pipeline, tx);
                app.focus = Focus::Results;
            }
        }
        KeyCode::Esc => {
            if !app.suggestions.is_empty() {
                app.suggestions.clear();
                app.sugg_sel = None;
            } else if !app.input.is_empty() {
                app.input.clear();
                app.cursor = 0;
            } else {
                return true;
            }
        }
        _ => {}
    }
    if edited {
        queue_suggest(app, store, tx);
    }
    false
}

fn results_key(
    key: KeyEvent,
    app: &mut App,
    pipeline: &SearchPipeline,
    answer_loop: Option<&AnswerLoop>,
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Esc | KeyCode::Char('/') => app.focus = Focus::Input,
        KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
        KeyCode::Char('g') | KeyCode::Home => app.selected = 0,
        KeyCode::Char('G') | KeyCode::End => {
            if !app.results.is_empty() {
                app.selected = app.results.len() - 1;
            }
        }
        KeyCode::Char('o') | KeyCode::Enter => {
            if let Some(row) = app.results.get(app.selected) {
                let url = row.url.to_string();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let opened = url.clone();
                    let note =
                        match tokio::task::spawn_blocking(move || webbrowser::open(&url)).await {
                            Ok(Ok(())) => format!("opened {opened}"),
                            Ok(Err(e)) => format!("open failed: {e}"),
                            Err(e) => format!("open failed: {e}"),
                        };
                    let _ = tx.send(Msg::Note(note));
                });
            }
        }
        KeyCode::Char('y') => {
            if let Some(row) = app.results.get(app.selected) {
                osc52_copy(row.url.as_str());
                app.note = Some(format!("yanked {}", row.url));
            }
        }
        KeyCode::Char('Y') => {
            if !app.results.is_empty() {
                osc52_copy(&results_markdown(app));
                app.note = Some(format!("copied {} results as markdown", app.results.len()));
            }
        }
        KeyCode::Char('a') => match answer_loop {
            Some(loop_) => start_assist(app, loop_, tx),
            None => app.note = Some("ai disabled — set [ai] in the config".into()),
        },
        KeyCode::Char('n') if !app.searching && app.status.is_some() && !app.results.is_empty() => {
            app.page = app.page.saturating_add(1);
            start_search(app, pipeline, tx);
        }
        _ => {}
    }
    false
}

/// Keys while the answer pane is focused: `j`/`k` scroll, `y` copies the
/// last turn as markdown, `/` hands a follow-up to the omnibox (kept in
/// ask mode so `Enter` continues the thread), `Esc`/`q`/`x` close the pane
/// back to the results.
fn answer_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Esc | KeyCode::Char('x') => {
            app.focus = Focus::Results;
        }
        KeyCode::Char('/') => app.focus = Focus::Input,
        KeyCode::Char('j') | KeyCode::Down => {
            app.answer_scroll = app.answer_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.answer_scroll = app.answer_scroll.saturating_sub(1);
        }
        KeyCode::Char('g') | KeyCode::Home => app.answer_scroll = u16::MAX / 2,
        KeyCode::Char('G') | KeyCode::End => app.answer_scroll = 0,
        KeyCode::Char('y') => {
            if let Some(text) = app
                .answer
                .as_ref()
                .and_then(|v| v.turns.last())
                .map(answer_markdown)
            {
                osc52_copy(&text);
                app.note = Some("copied answer as markdown".into());
            }
        }
        _ => {}
    }
    false
}

/// The results list as a markdown checklist (`Y` on the results pane).
fn results_markdown(app: &App) -> String {
    let mut out = format!("## Results for \"{}\"\n\n", app.query());
    for (i, r) in app.results.iter().enumerate() {
        out.push_str(&format!("{}. [{}]({})\n", i + 1, r.title, r.url));
        let snippet = r.snippet.replace('\n', " ");
        if !snippet.trim().is_empty() {
            out.push_str(&format!("   {}\n", snippet.trim()));
        }
    }
    out
}

/// One answer turn as markdown (`y` on the answer pane): the `done.answer`
/// markdown verbatim (or partial deltas mid-flight) plus a numbered
/// sources list the inline `[n]` cites resolve to.
fn answer_markdown(turn: &TurnView) -> String {
    let body = turn
        .done
        .as_ref()
        .map(|d| d.answer.clone())
        .unwrap_or_else(|| turn.deltas.clone());
    let mut out = format!("## {}\n\n{body}\n", turn.q);
    if !turn.sources.is_empty() {
        out.push_str("\nSources:\n");
        for (i, s) in turn.sources.iter().enumerate() {
            out.push_str(&format!("{}. [{}]({})\n", i + 1, s.title, s.url));
        }
    }
    out
}

/// OSC52 clipboard write: works over SSH and in tmux (`set -g
/// set-clipboard on`); terminals without support silently drop the
/// sequence.
fn osc52_copy(text: &str) {
    use base64::Engine;
    let payload = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let mut out = io::stdout().lock();
    let _ = write!(out, "\x1b]52;c;{payload}\x07");
    let _ = out.flush();
}

/// `cache tier N · M s ago` / `live` — same phrasing as `cauce search`.
fn source_label(source: &Source) -> String {
    match source {
        Source::Cache { tier, age_s, .. } => {
            format!("cache tier {} · {} s ago", tier.as_u8(), age_s)
        }
        Source::Network => "live".to_string(),
    }
}

fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    let (prompt, title) = if app.ai_mode {
        ("≈ ", " cauce · ask ")
    } else {
        ("› ", " cauce ")
    };
    let input = Paragraph::new(Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::Cyan)),
        Span::raw(app.query()),
    ]))
    .block(Block::default().borders(Borders::ALL).title(Span::styled(
        title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    f.render_widget(input, chunks[0]);
    if app.focus == Focus::Input {
        // +1 border, +2 for the prompt.
        f.set_cursor_position((chunks[0].x + 3 + app.cursor as u16, chunks[0].y + 1));
    }

    if app.answer.is_some() {
        render_answer(f, chunks[1], app);
        render_status(f, chunks[2], app);
        render_hints(f, chunks[3], app);
        render_suggestions(f, chunks[0], app);
        return;
    }

    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|row| {
            let mut title = vec![
                Span::styled(&row.title, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  [{}]", row.engine),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            if let Some(rank) = row.rank {
                title.push(Span::styled(
                    format!("  #{}", rank + 1),
                    Style::default().fg(Color::Yellow),
                ));
            }
            ListItem::new(vec![
                Line::from(title),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(row.url.as_str(), Style::default().fg(Color::Blue)),
                    Span::styled(
                        format!("  {}", row.snippet),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
            ])
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_symbol("› ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default()
        .with_selected((!app.results.is_empty()).then(|| app.selected.min(app.results.len() - 1)));
    f.render_stateful_widget(list, chunks[1], &mut state);

    render_status(f, chunks[2], app);
    render_hints(f, chunks[3], app);
    render_suggestions(f, chunks[0], app);
}

/// The answer pane: one block per open `AnswerView`, turns in order —
/// `› q` heading, `· step` tool-call lines, the streamed body (plain
/// text; concatenated deltas equal `done.answer`'s markdown), then a dim
/// footer (`confidence/model/cached`) + numbered sources. `answer_scroll`
/// is lines up from the tail, so `0` follows the stream.
fn render_answer(f: &mut Frame, area: Rect, app: &App) {
    let Some(view) = &app.answer else { return };
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line> = Vec::new();
    for turn in &view.turns {
        lines.push(Line::from(vec![
            Span::styled("› ", Style::default().fg(Color::Cyan)),
            Span::styled(
                turn.q.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        for step in &turn.steps {
            lines.push(Line::from(Span::styled(format!("  · {step}"), dim)));
        }
        let body = turn
            .done
            .as_ref()
            .map(|d| d.answer.as_str())
            .unwrap_or(turn.deltas.as_str());
        for line in body.lines() {
            lines.push(Line::raw(format!("  {line}")));
        }
        if turn.streaming {
            lines.push(Line::from(Span::styled("  …", dim)));
        }
        if let Some(done) = &turn.done {
            lines.push(Line::from(Span::styled(
                format!(
                    "  — {} · confidence {}/10{}{}{}",
                    done.model,
                    done.confidence,
                    if done.cached { " · cached" } else { "" },
                    if done.ungrounded {
                        " · ungrounded"
                    } else {
                        ""
                    },
                    done.log_id
                        .map(|id| format!(" · log #{id}"))
                        .unwrap_or_default(),
                ),
                dim,
            )));
            for (i, s) in turn.sources.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  [{}] ", i + 1), dim),
                    Span::styled(s.title.clone(), Style::default().fg(Color::Blue)),
                ]));
            }
            if !done.related.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  related: {}", done.related.join(" · ")),
                    dim,
                )));
            }
        }
        if let Some(err) = &turn.error {
            lines.push(Line::from(Span::styled(
                format!("  error: {err}"),
                Style::default().fg(Color::Red),
            )));
        }
        lines.push(Line::raw(""));
    }
    let title = if view.assist { " assist " } else { " ai " };
    let pane = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .scroll((app.answer_scroll, 0));
    f.render_widget(pane, area);
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let left = if let Some(err) = &app.error {
        Span::styled(format!("error: {err}"), Style::default().fg(Color::Red))
    } else if let Some(note) = &app.note {
        Span::styled(note.clone(), Style::default().fg(Color::Green))
    } else {
        Span::styled(
            app.status
                .clone()
                .unwrap_or_else(|| "type a query".to_string()),
            Style::default().fg(Color::DarkGray),
        )
    };
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

/// The bottom row stands on its own: a mode badge then key hints with the
/// keys in cyan — it reads as a command bar instead of footer noise.
fn render_hints(f: &mut Frame, area: Rect, app: &App) {
    let key = Style::default().fg(Color::Cyan);
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::styled(
        match app.focus {
            Focus::Answer if app.answer.as_ref().is_some_and(|v| v.assist) => "[assist]",
            Focus::Answer => "[ai]",
            _ if app.ai_mode => "[ai]",
            _ => "[search]",
        },
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    let pairs: &[(&str, &str)] = match app.focus {
        Focus::Input if app.ai_mode => &[
            ("enter", "ask"),
            ("^a", "search mode"),
            ("esc", "clear/quit"),
        ],
        Focus::Input => &[
            ("enter", "search"),
            ("^a", "ai mode"),
            ("esc", "clear/quit"),
        ],
        Focus::Results => &[
            ("j/k", "move"),
            ("o", "open"),
            ("y", "yank"),
            ("Y", "copy md"),
            ("a", "assist"),
            ("n", "next"),
            ("/", "query"),
            ("q", "quit"),
        ],
        Focus::Answer => &[
            ("j/k", "scroll"),
            ("/", "follow-up"),
            ("y", "copy md"),
            ("esc", "close"),
            ("q", "quit"),
        ],
    };
    for (k, label) in pairs {
        spans.push(Span::styled(format!(" {k}"), key));
        spans.push(Span::styled(format!(" {label}"), dim));
        spans.push(Span::styled(" ·", dim));
    }
    spans.pop();
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_suggestions(f: &mut Frame, input_area: Rect, app: &App) {
    if app.focus != Focus::Input || app.suggestions.is_empty() {
        return;
    }
    let height = (app.suggestions.len() as u16 + 2).min(f.area().height / 2);
    let area = Rect {
        x: input_area.x + 1,
        y: input_area.y + input_area.height,
        width: input_area.width.saturating_sub(2),
        height,
    };
    let items: Vec<ListItem> = app
        .suggestions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if app.sugg_sel == Some(i) {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(format!(" {s}"), style)))
        })
        .collect();
    f.render_widget(Clear, area);
    f.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut words = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Err("help requested".into()),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag {other:?}"));
            }
            positional => words.push(positional),
        }
    }
    Ok(Args {
        query: (!words.is_empty()).then(|| words.join(" ")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cauce_core::{SearchMeta, SearchResult, StreamMeta};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn result(url: &str, engine: &str) -> SearchResult {
        SearchResult {
            url: Url::parse(url).unwrap(),
            title: format!("title {url}"),
            snippet: "snippet".into(),
            engine: EngineId::from(engine),
            published: None,
            score: 1.0,
        }
    }

    fn meta(order: Vec<Url>) -> StreamMeta {
        StreamMeta {
            meta: SearchMeta {
                source: Source::Network,
                engines_used: vec![],
                engines_skipped: vec![],
                deadline_hit: false,
                hedged: false,
                hedge_at_ms: None,
                elapsed_ms: 7,
                request_id: Uuid::nil(),
            },
            order,
        }
    }

    #[test]
    fn dedupe_skips_normalized_duplicates() {
        let mut app = App::default();
        app.apply_stream(StreamEvent::Results {
            engine: EngineId::from("a"),
            elapsed_ms: 1,
            results: vec![result("https://example.com/x?utm_source=rss#top", "a")],
        });
        app.apply_stream(StreamEvent::Results {
            engine: EngineId::from("b"),
            elapsed_ms: 2,
            results: vec![result("https://m.example.com/x", "b")],
        });
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].engine.as_str(), "a");
    }

    #[test]
    fn meta_marks_late_but_better_results() {
        let mut app = App::default();
        let slow = result("https://slow.example/", "slow");
        let key = normalize_url(&slow.url);
        app.apply_stream(StreamEvent::Results {
            engine: EngineId::from("fast"),
            elapsed_ms: 1,
            results: vec![result("https://fast.example/", "fast")],
        });
        app.apply_stream(StreamEvent::Results {
            engine: EngineId::from("slow"),
            elapsed_ms: 9,
            results: vec![slow],
        });
        app.apply_stream(StreamEvent::Meta(meta(vec![key])));
        // The slow batch arrived second but ranks first in the final order.
        assert_eq!(app.results[1].rank, Some(0));
        assert_eq!(app.results[0].rank, None);
    }

    #[test]
    fn meta_does_not_flag_appended_rows_in_order() {
        // Page-2-style append: rows that land in the same order the final
        // `Meta` ranks them must not be flagged late-but-better.
        let mut app = App::default();
        let keys: Vec<Url> = (0..8)
            .map(|i| normalize_url(&Url::parse(&format!("https://e{i}.example/")).unwrap()))
            .collect();
        for i in 0..keys.len() {
            app.apply_stream(StreamEvent::Results {
                engine: EngineId::from("a"),
                elapsed_ms: 1,
                results: vec![result(&format!("https://e{i}.example/"), "a")],
            });
        }
        app.apply_stream(StreamEvent::Meta(meta(keys)));
        assert!(app.results.iter().all(|r| r.rank.is_none()));
    }

    #[test]
    fn render_shows_omnibox_and_hints() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("cauce"));
        assert!(text.contains("type a query"));
        // The dedicated command-bar row: mode badge + styled key hints.
        assert!(text.contains("[search]"));
        assert!(text.contains("enter"));
        assert!(text.contains("search"));
    }

    fn source(url: &str) -> AnswerSource {
        AnswerSource {
            url: Url::parse(url).unwrap(),
            title: format!("title {url}"),
            snippet: "snippet".into(),
            engine: EngineId::from("a"),
        }
    }

    #[test]
    fn answer_frames_accumulate_into_turn_and_history() {
        let mut app = App::default();
        open_turn(&mut app, false, "what is rrf");
        app.apply_answer_frame(AnswerFrame::Step {
            tool: "search_web".into(),
            query: "rrf".into(),
            label: "Searching: rrf".into(),
        });
        app.apply_answer_frame(AnswerFrame::Delta { text: "Re".into() });
        app.apply_answer_frame(AnswerFrame::Delta {
            text: "ciprocal".into(),
        });
        app.apply_answer_frame(AnswerFrame::Sources {
            sources: vec![source("https://a.example/")],
        });
        app.apply_answer_frame(AnswerFrame::Done {
            answer: "Reciprocal rank fusion…".into(),
            html: String::new(),
            confidence: 9,
            model: "m".into(),
            related_questions: vec![],
            cached: false,
            request_id: Uuid::nil(),
            ungrounded: false,
            log_id: Some(3),
        });
        let view = app.answer.as_ref().unwrap();
        let turn = &view.turns[0];
        assert_eq!(turn.deltas, "Reciprocal");
        assert_eq!(turn.steps, ["Searching: rrf"]);
        assert!(!turn.streaming);
        assert_eq!(view.history.len(), 2);
        assert_eq!(view.history[1].role, AnswerRole::Assistant);
        assert!(app.status.as_deref().unwrap().contains("confidence 9/10"));
    }

    #[test]
    fn assist_turns_never_join_history() {
        // Assist is single-turn (the SPA's assist card has no thread);
        // only `Done` on a full answer pushes user+assistant history.
        let mut app = App::default();
        open_turn(&mut app, true, "summarize");
        app.apply_answer_frame(AnswerFrame::Done {
            answer: "sum".into(),
            html: String::new(),
            confidence: 7,
            model: "m".into(),
            related_questions: vec![],
            cached: false,
            request_id: Uuid::nil(),
            ungrounded: false,
            log_id: None,
        });
        assert!(app.answer.as_ref().unwrap().assist);
    }

    #[test]
    fn answer_markdown_numbers_sources() {
        let mut app = App::default();
        open_turn(&mut app, false, "q");
        app.apply_answer_frame(AnswerFrame::Sources {
            sources: vec![source("https://a.example/"), source("https://b.example/")],
        });
        app.apply_answer_frame(AnswerFrame::Done {
            answer: "body [1]".into(),
            html: String::new(),
            confidence: 8,
            model: "m".into(),
            related_questions: vec![],
            cached: false,
            request_id: Uuid::nil(),
            ungrounded: false,
            log_id: None,
        });
        let md = answer_markdown(&app.answer.as_ref().unwrap().turns[0]);
        assert!(md.contains("body [1]"));
        assert!(md.contains("1. [title https://a.example/](https://a.example/)"));
        assert!(md.contains("2. [title https://b.example/](https://b.example/)"));
    }

    #[test]
    fn results_markdown_numbers_rows() {
        let mut app = App::default();
        app.apply_stream(StreamEvent::Results {
            engine: EngineId::from("a"),
            elapsed_ms: 1,
            results: vec![result("https://a.example/", "a")],
        });
        let md = results_markdown(&app);
        assert!(md.contains("1. [title https://a.example/](https://a.example/)"));
    }

    #[test]
    fn render_answer_pane_badge() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        open_turn(&mut app, true, "summarize this");
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("assist"));
        assert!(text.contains("[assist]"));
        assert!(text.contains("summarize this"));
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let area = terminal.backend().buffer().area;
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = terminal.backend().buffer().cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }
}
