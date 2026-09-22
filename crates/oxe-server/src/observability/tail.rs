//! `oxe tail` support: turn JSONL log records into pretty terminal lines.
//!
//! One line per `event` record; each `span_open`/`span_close` pair collapses
//! into a single line emitted at close time (`engine=bing 640ms ok`). A span
//! that closes with `status` `error`/`timeout` (or an `error` field) is
//! reported at WARN, so `oxe tail --level warn` still surfaces engine
//! failures even though engine spans are declared at INFO.
//!
//! The file side (listing log files, parsing records) is shared with
//! `oxe trace` in `super::trace`; this module only filters and renders.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::trace::{LogRecord, SpanPart};

/// Severity rank for level filtering (`trace` < `debug` < `info` < `warn` <
/// `error`). Unknown level strings rank as `info`.
fn level_rank(level: &str) -> u8 {
    match level.to_ascii_lowercase().as_str() {
        "trace" => 0,
        "debug" => 1,
        "warn" | "warning" => 3,
        "error" => 4,
        // `info` and anything unexpected.
        _ => 2,
    }
}

/// Level names by rank, for the level column. A failed span prints at WARN
/// (see module docs), so the name must be derived from the effective rank.
const LEVEL_NAMES: [&str; 5] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"];

/// ANSI colour per rank: error bold red, warn yellow, info green, debug
/// blue, trace dim.
const LEVEL_STYLES: [&str; 5] = ["2", "34", "32", "33", "1;31"];

/// Whether `s` is a usable `--level` value (CLI flag validation).
pub fn valid_level(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "trace" | "debug" | "info" | "warn" | "error"
    )
}

/// Display filters for [`Tail`]; populated from `oxe tail` flags.
#[derive(Debug, Clone, Default)]
pub struct TailFilter {
    /// `--request <id>`: keep records whose `request_id` starts with this
    /// (prefix match, so the 8-char column form works).
    pub request: Option<String>,
    /// `--level <lvl>`: minimum severity; `None` shows everything.
    pub level: Option<String>,
    /// `--engine <id>`: keep records carrying `engine=<id>` — on the record's
    /// own fields, on its span's fields, or inherited from an ancestor span
    /// (so events inside the engine's `http` span also match).
    pub engine: Option<String>,
}

/// Stateful renderer: feed it [`LogRecord`]s in file order, print the
/// returned lines. Keeps two maps: span id -> engine name (resolved on
/// `span_open`, inherited from the parent) for `--engine`, and span id ->
/// open record so [`Tail::finish`] can report spans still open at EOF.
pub struct Tail {
    filter: TailFilter,
    /// ANSI colours on/off (TTY + `NO_COLOR` decided by the caller).
    color: bool,
    span_engine: BTreeMap<u64, String>,
    open: BTreeMap<u64, LogRecord>,
}

impl Tail {
    pub fn new(filter: TailFilter, color: bool) -> Self {
        Self {
            filter,
            color,
            span_engine: BTreeMap::new(),
            open: BTreeMap::new(),
        }
    }

    /// Consume one record; returns the line to print, or `None` for records
    /// the filters drop and for `span_open` (its line arrives at close).
    pub fn push(&mut self, record: &LogRecord) -> Option<String> {
        match record.kind.as_str() {
            "span_open" => {
                if let Some(span) = &record.span {
                    let engine = span
                        .fields
                        .get("engine")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| span.parent.and_then(|p| self.span_engine.get(&p).cloned()));
                    if let Some(engine) = engine {
                        self.span_engine.insert(span.id, engine);
                    }
                    self.open.insert(span.id, record.clone());
                }
                None
            }
            "span_close" => {
                let span = record.span.clone()?;
                self.open.remove(&span.id);
                let status = status_of(&span.fields);
                let rank = effective_rank(&record.level, &status, &span.fields);
                let line = self
                    .keep(record, rank, Some(&span))
                    .then(|| self.render_span_close(record, &span, &status, rank));
                self.span_engine.remove(&span.id);
                line
            }
            _ => {
                let rank = level_rank(&record.level);
                self.keep(record, rank, record.span.as_ref())
                    .then(|| self.render_event(record))
            }
        }
    }

    /// Lines for spans still open at end of input (non-follow mode). In
    /// `--follow` this is never called: pending spans print when they close.
    pub fn finish(&mut self) -> Vec<String> {
        let open = std::mem::take(&mut self.open);
        open.values()
            .filter(|r| {
                let rank = level_rank(&r.level);
                self.keep(r, rank, r.span.as_ref())
            })
            .filter_map(|r| r.span.as_ref().map(|s| self.render_open(r, s)))
            .collect()
    }

    fn keep(&self, r: &LogRecord, rank: u8, span: Option<&SpanPart>) -> bool {
        if let Some(want) = &self.filter.request
            && r.request_id
                .as_deref()
                .is_none_or(|id| !id.starts_with(want.as_str()))
        {
            return false;
        }
        if let Some(min) = &self.filter.level
            && rank < level_rank(min)
        {
            return false;
        }
        if let Some(want) = &self.filter.engine {
            let own = r.fields.get("engine").and_then(Value::as_str);
            let via_span = span
                .and_then(|s| s.fields.get("engine"))
                .and_then(Value::as_str)
                .or_else(|| {
                    span.and_then(|s| self.span_engine.get(&s.id))
                        .map(String::as_str)
                });
            if own != Some(want.as_str()) && via_span != Some(want.as_str()) {
                return false;
            }
        }
        true
    }

    fn dim(&self, s: impl AsRef<str>) -> String {
        self.paint("2", s.as_ref())
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    /// `{ts} {LEVEL} {reqid8}` shared prefix.
    fn prefix(&self, r: &LogRecord, rank: u8) -> String {
        let ts =
            r.ts.map(|t| t.format("%H:%M:%S%.3f").to_string())
                .unwrap_or_else(|| "           -".to_string());
        let level = format!("{:<5}", LEVEL_NAMES[rank.min(4) as usize]);
        let rid = r
            .request_id
            .as_deref()
            .map(|id| id.chars().take(8).collect::<String>())
            .unwrap_or_else(|| "-".to_string());
        format!(
            "{} {} {}",
            self.dim(ts),
            self.paint(LEVEL_STYLES[rank.min(4) as usize], &level),
            self.dim(rid)
        )
    }

    /// `hit=false tier=1`-style field list; `request_id` is already in the
    /// prefix column.
    fn fmt_fields(fields: &Map<String, Value>) -> String {
        fields
            .iter()
            .filter(|(k, _)| k.as_str() != "request_id")
            .map(|(k, v)| match v {
                Value::String(s) => format!("{k}={s}"),
                other => format!("{k}={other}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn render_event(&self, r: &LogRecord) -> String {
        let mut fields = r.fields.clone();
        let message = fields
            .remove("message")
            .and_then(|v| v.as_str().map(str::to_owned));
        let mut summary = message.unwrap_or_default();
        let rest = Self::fmt_fields(&fields);
        if !rest.is_empty() {
            if !summary.is_empty() {
                summary.push(' ');
            }
            summary.push_str(&rest);
        }
        format!(
            "{} {} {}",
            self.prefix(r, level_rank(&r.level)),
            summary,
            self.dim(format!("({})", r.target))
        )
    }

    fn render_span_close(&self, r: &LogRecord, span: &SpanPart, status: &str, rank: u8) -> String {
        let mut fields = span.fields.clone();
        fields.remove("status");
        let head = {
            let f = Self::fmt_fields(&fields);
            if f.is_empty() { span.name.clone() } else { f }
        };
        let ms = r.busy_ms.map(fmt_ms).unwrap_or_else(|| "…".to_string());
        let status_style = match status {
            "ok" => "32",
            "error" | "timeout" => "1;31",
            _ => "33",
        };
        format!(
            "{} {} {} {}",
            self.prefix(r, rank),
            head,
            ms,
            self.paint(status_style, status)
        )
    }

    fn render_open(&self, r: &LogRecord, span: &SpanPart) -> String {
        let f = Self::fmt_fields(&span.fields);
        let head = if f.is_empty() { span.name.clone() } else { f };
        format!(
            "{} {} {} {}",
            self.prefix(r, level_rank(&r.level)),
            head,
            "…",
            self.dim("open")
        )
    }
}

/// Trailing status word for a collapsed span: the recorded `status` field
/// (`ok`/`error`/`timeout`), `error` when only an `error` field exists, else
/// `ok`.
fn status_of(fields: &Map<String, Value>) -> String {
    if let Some(s) = fields.get("status").and_then(Value::as_str) {
        return s.to_string();
    }
    if fields.contains_key("error") {
        "error".to_string()
    } else {
        "ok".to_string()
    }
}

/// Effective severity of a span close: the span's own level, bumped to WARN
/// when it did not succeed so `--level warn` keeps engine failures visible.
fn effective_rank(level: &str, status: &str, fields: &Map<String, Value>) -> u8 {
    let rank = level_rank(level);
    if !matches!(status, "ok") || fields.contains_key("error") {
        rank.max(level_rank("warn"))
    } else {
        rank
    }
}

/// `640ms`, `640.4ms`; integers print without decimals.
fn fmt_ms(ms: f64) -> String {
    if ms.fract().abs() < 0.05 {
        format!("{ms:.0}ms")
    } else {
        format!("{ms:.1}ms")
    }
}
