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
//! in browser, `y` yank via OSC52, `n` next page, `/` back to the omnibox,
//! `q`/`Ctrl-C` quit.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use cauce_core::config::{Config, Resources};
use cauce_core::{
    Admission, AdmissionLimits, CachePolicy, ClientKind, EngineId, HealthPolicy, HedgePolicy,
    MergePolicy, SafeSearch, SearchOpts, SearchPipeline, SearchRequest, Source, Store, StreamEvent,
    normalize_url,
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
    let code = run_app(&mut terminal, &pipeline, &store, opts.query).await;
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
    Note(String),
}

/// Focus target for key routing: the omnibox owns typing until a search
/// runs; `/` and `Esc` move focus back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Focus {
    #[default]
    Input,
    Results,
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
        if handle(msg, &mut app, pipeline, store, &tx) {
            break;
        }
        while let Ok(msg) = rx.try_recv() {
            if handle(msg, &mut app, pipeline, store, &tx) {
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
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    match msg {
        Msg::Term(Event::Key(key)) => return handle_key(key, app, pipeline, store, tx),
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
    tx: &mpsc::UnboundedSender<Msg>,
) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    app.note = None;
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'));
    }
    match app.focus {
        Focus::Input => input_key(key, app, pipeline, store, tx),
        Focus::Results => results_key(key, app, pipeline, tx),
    }
}

fn input_key(
    key: KeyEvent,
    app: &mut App,
    pipeline: &SearchPipeline,
    store: &Arc<SqliteStore>,
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
            app.results.clear();
            app.seen.clear();
            app.page = 1;
            start_search(app, pipeline, tx);
            app.focus = Focus::Results;
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
        KeyCode::Char('n') if !app.searching && app.status.is_some() && !app.results.is_empty() => {
            app.page = app.page.saturating_add(1);
            start_search(app, pipeline, tx);
        }
        _ => {}
    }
    false
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
        ])
        .split(f.area());

    let input = Paragraph::new(Line::from(vec![
        Span::styled("› ", Style::default().fg(Color::Cyan)),
        Span::raw(app.query()),
    ]))
    .block(Block::default().borders(Borders::ALL).title(Span::styled(
        " cauce ",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    f.render_widget(input, chunks[0]);
    if app.focus == Focus::Input {
        // +1 border, +2 for the prompt.
        f.set_cursor_position((chunks[0].x + 3 + app.cursor as u16, chunks[0].y + 1));
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
    render_suggestions(f, chunks[0], app);
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
    let hints = match app.focus {
        Focus::Input => "enter: search · esc: clear/quit",
        Focus::Results => "j/k: move · o: open · y: yank · n: next · /: query · q: quit",
    };
    let pad = area
        .width
        .saturating_sub(left.width() as u16 + hints.len() as u16 + 1);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            left,
            Span::raw(" ".repeat(pad as usize)),
            Span::styled(hints, Style::default().fg(Color::DarkGray)),
        ])),
        area,
    );
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
        assert!(text.contains("enter: search"));
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
