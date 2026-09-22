//! `oxe trace <request_id>`: read the JSONL logs newest file first, keep
//! records whose `request_id` matches, render an ordered timeline of the
//! span tree (engine spans with durations, cache tier decisions, errors).
//!
//! The reader is deliberately dependency-free so the W2 `/trace/{id}` page
//! can reuse [`trace_request`] / [`render_trace`] verbatim.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};

/// One parsed JSONL record (schema `v = 1`, see `super::jsonl`).
#[derive(Debug, Clone, Deserialize)]
pub struct LogRecord {
    #[serde(default)]
    pub kind: String,
    pub ts: Option<DateTime<Utc>>,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub span: Option<SpanPart>,
    #[serde(default)]
    pub spans: Vec<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub busy_ms: Option<f64>,
}

/// The `span` object on a record.
#[derive(Debug, Clone, Deserialize)]
pub struct SpanPart {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub parent: Option<u64>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

/// Errors from the trace reader.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error("cannot read logs dir {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("invalid request id {0:?}: {1}")]
    BadId(String, String),
}

/// Log files matching the writer's naming, newest first. Accepts both the
/// canonical `oxe-YYYY-MM-DD.jsonl` and the raw `oxe.YYYY-MM-DD.jsonl` the
/// appender produces before canonicalisation.
pub fn log_files(logs_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(logs_dir)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", logs_dir.display())))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("oxe") && n.ends_with(".jsonl"))
        })
        .collect();
    // ISO dates in the name sort lexicographically.
    files.sort();
    files.reverse();
    Ok(files)
}

/// All records carrying `request_id`, ordered by timestamp. Scans files
/// newest first; a request never outlives the retention window so missing
/// earlier files is not an error.
pub fn trace_request(logs_dir: &Path, request_id: &str) -> Result<Vec<LogRecord>, TraceError> {
    let id = request_id.trim();
    if id.is_empty() {
        return Err(TraceError::BadId(request_id.into(), "empty".into()));
    }
    let files = log_files(logs_dir).map_err(|e| TraceError::Io(logs_dir.to_path_buf(), e))?;
    let needle = format!("\"{id}\"");
    let mut records = Vec::new();
    for file in files {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        for line in content.lines() {
            if !line.contains(&needle) {
                continue;
            }
            let Ok(record) = serde_json::from_str::<LogRecord>(line) else {
                continue;
            };
            if record.request_id.as_deref() == Some(id) {
                records.push(record);
            }
        }
    }
    records.sort_by_key(|r| r.ts);
    Ok(records)
}

/// A span node reconstructed from `span_open`/`span_close` records.
#[derive(Debug, Default)]
struct SpanNode {
    name: String,
    fields: Map<String, Value>,
    parent: Option<u64>,
    open_ts: Option<DateTime<Utc>>,
    busy_ms: Option<f64>,
    /// Child spans keyed by id and events, interleaved by timestamp.
    children: Vec<TimelineItem>,
}

#[derive(Debug)]
enum TimelineItem {
    Span(u64),
    Event(Box<LogRecord>),
}

impl TimelineItem {
    fn ts(&self, nodes: &BTreeMap<u64, SpanNode>) -> Option<DateTime<Utc>> {
        match self {
            Self::Span(id) => nodes.get(id).and_then(|n| n.open_ts),
            Self::Event(r) => r.ts,
        }
    }
}

/// Render the human-readable timeline `oxe trace` prints.
///
/// ```text
/// trace 0194… (9 records)
/// 10:00:00.000 request  request_id=0194… client=api   43.1 ms
/// 10:00:00.001   INFO  cache_lookup hit=false tier=1  (oxe_core::pipeline)
/// 10:00:00.002   engine  engine=ddgs                    12.4 ms
/// ```
pub fn render_trace(request_id: &str, records: &[LogRecord]) -> String {
    let mut nodes: BTreeMap<u64, SpanNode> = BTreeMap::new();
    // Order of first appearance for spans with no usable timestamp.
    let mut roots: Vec<TimelineItem> = Vec::new();

    for record in records {
        match record.kind.as_str() {
            "span_open" | "span_close" => {
                let Some(span) = &record.span else { continue };
                let node = nodes.entry(span.id).or_insert_with(|| SpanNode {
                    name: span.name.clone(),
                    ..SpanNode::default()
                });
                if node.name.is_empty() {
                    node.name = span.name.clone();
                }
                if !span.fields.is_empty() {
                    node.fields.extend(span.fields.clone());
                }
                if let Some(parent) = span.parent {
                    node.parent = Some(parent);
                }
                if record.kind == "span_open" {
                    node.open_ts = record.ts;
                    if let Some(parent) = node.parent {
                        nodes
                            .entry(parent)
                            .or_default()
                            .children
                            .push(TimelineItem::Span(span.id));
                    } else {
                        roots.push(TimelineItem::Span(span.id));
                    }
                } else {
                    node.busy_ms = record.busy_ms;
                }
            }
            _ => {
                let attached = record
                    .span
                    .as_ref()
                    .is_some_and(|s| nodes.contains_key(&s.id));
                match (attached, record.span.as_ref()) {
                    (true, Some(span)) => nodes
                        .entry(span.id)
                        .or_default()
                        .children
                        .push(TimelineItem::Event(Box::new(record.clone()))),
                    _ => roots.push(TimelineItem::Event(Box::new(record.clone()))),
                }
            }
        }
    }

    let mut out = format!("trace {request_id} ({} records)\n", records.len());
    if records.is_empty() {
        out.push_str("no records found\n");
        return out;
    }

    let mut lines = Vec::new();
    for item in &roots {
        render_item(item, &nodes, 0, &mut lines);
    }
    // Orphan spans whose parent id was never seen (e.g. a root recorded
    // with a parent id that predates the log files).
    let linked: std::collections::BTreeSet<u64> = nodes
        .values()
        .flat_map(|n| {
            n.children
                .iter()
                .filter_map(|c| match c {
                    TimelineItem::Span(id) => Some(*id),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let rooted: std::collections::BTreeSet<u64> = roots
        .iter()
        .filter_map(|i| match i {
            TimelineItem::Span(id) => Some(*id),
            _ => None,
        })
        .collect();
    for (id, node) in &nodes {
        if !linked.contains(id) && !rooted.contains(id) {
            render_span_line(node, 0, &mut lines);
            render_children(node, &nodes, 1, &mut lines);
        }
    }
    out.push_str(&lines.join(""));
    out
}

fn render_item(
    item: &TimelineItem,
    nodes: &BTreeMap<u64, SpanNode>,
    depth: usize,
    out: &mut Vec<String>,
) {
    match item {
        TimelineItem::Span(id) => {
            if let Some(node) = nodes.get(id) {
                render_span_line(node, depth, out);
                render_children(node, nodes, depth + 1, out);
            }
        }
        TimelineItem::Event(record) => render_event_line(record, depth, out),
    }
}

fn render_children(
    node: &SpanNode,
    nodes: &BTreeMap<u64, SpanNode>,
    depth: usize,
    out: &mut Vec<String>,
) {
    let mut children: Vec<&TimelineItem> = node.children.iter().collect();
    children.sort_by_key(|c| c.ts(nodes));
    for child in children {
        render_item(child, nodes, depth, out);
    }
}

fn ts_prefix(ts: Option<DateTime<Utc>>) -> String {
    ts.map(|t| t.format("%H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "          -".to_string())
}

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

fn render_span_line(node: &SpanNode, depth: usize, out: &mut Vec<String>) {
    // Placeholder nodes exist only to hang children on (the parent's own
    // span_open was never logged for this request); skip their header.
    if node.name.is_empty() && node.open_ts.is_none() {
        return;
    }
    let indent = "  ".repeat(depth);
    let fields = fmt_fields(&node.fields);
    let duration = node
        .busy_ms
        .map(|ms| format!("{ms:.1} ms"))
        .unwrap_or_else(|| "…".to_string());
    out.push(format!(
        "{}  {indent}{}{}  {duration}\n",
        ts_prefix(node.open_ts),
        node.name,
        if fields.is_empty() {
            String::new()
        } else {
            format!("  {fields}")
        }
    ));
}

fn render_event_line(record: &LogRecord, depth: usize, out: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let mut fields = record.fields.clone();
    let message = fields
        .remove("message")
        .and_then(|v| v.as_str().map(str::to_owned));
    let mut summary = message.unwrap_or_default();
    let rest = fmt_fields(&fields);
    if !rest.is_empty() {
        if !summary.is_empty() {
            summary.push(' ');
        }
        summary.push_str(&rest);
    }
    out.push(format!(
        "{}  {indent}{:<5} {}  ({})\n",
        ts_prefix(record.ts),
        record.level,
        summary,
        record.target
    ));
}
