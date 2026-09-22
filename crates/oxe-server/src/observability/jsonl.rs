//! JSON-lines `tracing` layer writing `logs/oxe-YYYY-MM-DD.jsonl`.
//!
//! One line per record, three record kinds:
//!
//! - `event`: a `tracing` event. Carries `fields` (including `message`),
//!   the innermost `span` it happened in and the full `spans` path.
//! - `span_open`: emitted when a span is created; `span` carries the span
//!   id, parent id, name and declared fields.
//! - `span_close`: emitted when a span drops; adds `busy_ms` measured from
//!   span creation.
//!
//! Every line carries `v`, `kind`, `ts` (RFC 3339 UTC), `level`, `target`
//! and `request_id` (null when outside a request span). `request_id` is
//! hoisted from the nearest enclosing span field, which is what makes
//! `oxe trace <id>` a filter over whole files.
//!
//! Filename: `tracing-appender` produces `oxe.<date>.jsonl`
//! (`{prefix}.{date}.{suffix}` is baked into its builder), so a thin
//! `Write` wrapper renames `oxe.<date>.jsonl` to `oxe-<date>.jsonl` ahead of
//! writes, throttled to once a minute. Renaming is safe while the appender
//! holds the file open (the descriptor follows the inode), and retention
//! pruning still matches renamed files on the `oxe` prefix / `jsonl`
//! suffix. Readers must accept both spellings.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chrono::{SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::{LookupSpan, SpanRef};

/// Field name every request span records its `RequestId` under.
pub const REQUEST_ID_FIELD: &str = "request_id";

/// Schema version of the emitted JSONL records.
const SCHEMA_VERSION: u64 = 1;

/// How often the writer checks for `oxe.<date>.jsonl` files to canonicalise.
const NORMALIZE_INTERVAL_SECS: u64 = 60;

/// Create the non-blocking daily writer plus its flush guard.
///
/// Daily rotation and retention (`retention_days` files) come from
/// `tracing-appender`; `DailyJsonlWriter` only canonicalises file names.
pub fn daily_file_writer(dir: &Path, retention_days: usize) -> (NonBlocking, WorkerGuard) {
    // The appender prunes before it creates the directory, which would
    // otherwise print a spurious read_dir error on first init.
    if let Err(e) = fs::create_dir_all(dir) {
        panic!("cannot create log directory {}: {e}", dir.display());
    }
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("oxe")
        .filename_suffix("jsonl")
        .max_log_files(retention_days.max(1))
        .build(dir)
        .unwrap_or_else(|e| panic!("cannot create log file in {}: {e}", dir.display()));
    normalize_log_names(dir);
    tracing_appender::non_blocking(DailyJsonlWriter::new(appender, dir))
}

/// Rename `oxe.<YYYY-MM-DD>.jsonl` files to `oxe-<YYYY-MM-DD>.jsonl`.
///
/// Skips the rename when the canonical name already exists (a same-day
/// restart leaves both files; the trace reader globs both spellings).
/// Errors are ignored: worst case the dotted name stays.
pub fn normalize_log_names(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("oxe.") else {
            continue;
        };
        let Some(date) = rest.strip_suffix(".jsonl") else {
            continue;
        };
        if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            continue;
        }
        let target = dir.join(format!("oxe-{date}.jsonl"));
        if !target.exists() {
            let _ = fs::rename(entry.path(), &target);
        }
    }
}

/// `io::Write` adapter around the rolling appender that canonicalises file
/// names at most once per `NORMALIZE_INTERVAL_SECS`.
struct DailyJsonlWriter {
    appender: RollingFileAppender,
    dir: PathBuf,
    /// Unix seconds of the last normalize pass.
    last_normalize: u64,
}

impl DailyJsonlWriter {
    fn new(appender: RollingFileAppender, dir: &Path) -> Self {
        Self {
            appender,
            dir: dir.to_path_buf(),
            last_normalize: 0,
        }
    }

    fn maybe_normalize(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now.saturating_sub(self.last_normalize) >= NORMALIZE_INTERVAL_SECS {
            self.last_normalize = now;
            normalize_log_names(&self.dir);
        }
    }
}

impl Write for DailyJsonlWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.maybe_normalize();
        self.appender.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.appender.flush()
    }
}

/// Per-span data stored in registry extensions.
struct SpanData {
    /// Accumulated span fields (`request_id` lives here).
    fields: Map<String, Value>,
    /// Parent span id (`Id::into_u64`), `None` for roots.
    parent: Option<u64>,
    /// Wall-clock creation, for `busy_ms` on close.
    opened_at: Instant,
}

/// The JSONL layer itself: one object per event/span open/span close.
pub struct JsonlLayer {
    writer: Mutex<NonBlocking>,
}

impl JsonlLayer {
    pub fn new(writer: NonBlocking) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }

    fn emit(&self, record: Value) {
        let mut line = serde_json::to_string(&record).unwrap_or_else(|_| "{}".to_string());
        line.push('\n');
        // One `write_all` per line: `NonBlocking::write` enqueues the whole
        // buffer as one message, so lines never interleave.
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(line.as_bytes());
        }
    }
}

/// Convert recorded span/event fields into a JSON object.
struct JsonVisitor<'a> {
    map: &'a mut Map<String, Value>,
}

impl Visit for JsonVisitor<'_> {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.map.insert(field.name().to_string(), json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.map.insert(field.name().to_string(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.map.insert(field.name().to_string(), json!(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.map
            .insert(field.name().to_string(), json!(value.to_string()));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.map
            .insert(field.name().to_string(), json!(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.map.insert(field.name().to_string(), json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.map.insert(field.name().to_string(), json!(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.map
            .insert(field.name().to_string(), json!(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.map
            .insert(field.name().to_string(), json!(format!("{value:?}")));
    }
}

fn now_ts() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn level_str(meta: &Metadata<'_>) -> &'static str {
    meta.level().as_str()
}

/// `request_id` hoisted from a span's own fields first (the root request
/// span introduces it), then from the enclosing scope.
fn request_id_for<S>(
    own_fields: &Map<String, Value>,
    scope: Option<tracing_subscriber::registry::Scope<'_, S>>,
) -> Value
where
    S: for<'a> LookupSpan<'a>,
{
    if let Some(v) = own_fields.get(REQUEST_ID_FIELD) {
        return v.clone();
    }
    if let Some(scope) = scope {
        for span in scope {
            if let Some(data) = span.extensions().get::<SpanData>()
                && let Some(v) = data.fields.get(REQUEST_ID_FIELD)
            {
                return v.clone();
            }
        }
    }
    Value::Null
}

/// Innermost-first scope (leaf to root) from an event, for `request_id`.
fn event_scope<'a, 'b, S>(
    ctx: &'b Context<'a, S>,
    event: &Event<'_>,
) -> Option<tracing_subscriber::registry::Scope<'b, S>>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    ctx.event_span(event)
        .map(|span| span.scope())
        .or_else(|| ctx.lookup_current().map(|span| span.scope()))
}

/// Names of the enclosing span chain, root to leaf.
fn span_path<S>(scope: tracing_subscriber::registry::Scope<'_, S>) -> Vec<String>
where
    S: for<'a> LookupSpan<'a>,
{
    scope.from_root().map(|s| s.name().to_string()).collect()
}

fn span_json<S>(span: &SpanRef<'_, S>) -> Value
where
    S: for<'a> LookupSpan<'a>,
{
    let ext = span.extensions();
    let data = ext.get::<SpanData>();
    json!({
        "id": span.id().into_u64(),
        "name": span.name(),
        "parent": data.and_then(|d| d.parent),
        "fields": data.map(|d| d.fields.clone()).unwrap_or_default(),
    })
}

impl<S> Layer<S> for JsonlLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Map::new();
        attrs.record(&mut JsonVisitor { map: &mut fields });

        let parent = attrs.parent().map(Id::into_u64).or_else(|| {
            if attrs.is_contextual() {
                ctx.lookup_current().map(|s| s.id().into_u64())
            } else {
                None
            }
        });

        let meta = attrs.metadata();
        let Some(span) = ctx.span(id) else { return };
        span.extensions_mut().insert(SpanData {
            fields: fields.clone(),
            parent,
            opened_at: Instant::now(),
        });

        let request_id = request_id_for::<S>(&fields, Some(span.scope()));
        self.emit(json!({
            "v": SCHEMA_VERSION,
            "kind": "span_open",
            "ts": now_ts(),
            "level": level_str(meta),
            "target": meta.target(),
            "request_id": request_id,
            "span": span_json(&span),
            "spans": span_path(span.scope()),
        }));
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if let Some(data) = ext.get_mut::<SpanData>() {
                values.record(&mut JsonVisitor {
                    map: &mut data.fields,
                });
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Map::new();
        event.record(&mut JsonVisitor { map: &mut fields });

        // An event's own `request_id` field (e.g. `audit` outside a span)
        // wins over the enclosing scope.
        let scope = event_scope(&ctx, event);
        let request_id = request_id_for::<S>(&fields, scope);
        let span = ctx
            .event_span(event)
            .or_else(|| ctx.lookup_current())
            .map(|s| {
                json!({
                    "id": s.id().into_u64(),
                    "name": s.name(),
                })
            })
            .unwrap_or(Value::Null);
        let spans = ctx
            .event_span(event)
            .or_else(|| ctx.lookup_current())
            .map(|s| span_path(s.scope()))
            .unwrap_or_default();

        let meta = event.metadata();
        self.emit(json!({
            "v": SCHEMA_VERSION,
            "kind": "event",
            "ts": now_ts(),
            "level": level_str(meta),
            "target": meta.target(),
            "request_id": request_id,
            "span": span,
            "spans": spans,
            "fields": fields,
        }));
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let (fields, parent, busy_ms) = {
            let ext = span.extensions();
            match ext.get::<SpanData>() {
                Some(data) => (
                    data.fields.clone(),
                    data.parent,
                    data.opened_at.elapsed().as_secs_f64() * 1000.0,
                ),
                None => (Map::new(), None, 0.0),
            }
        };
        let meta = span.metadata();
        let request_id = request_id_for::<S>(&fields, Some(span.scope()));
        self.emit(json!({
            "v": SCHEMA_VERSION,
            "kind": "span_close",
            "ts": now_ts(),
            "level": level_str(meta),
            "target": meta.target(),
            "request_id": request_id,
            "span": {
                "id": id.into_u64(),
                "name": meta.name(),
                "parent": parent,
                "fields": fields,
            },
            "spans": span_path(span.scope()),
            "busy_ms": (busy_ms * 1000.0).round() / 1000.0,
        }));
    }
}
