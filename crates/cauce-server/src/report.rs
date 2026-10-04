//! The v1 report collector (#239): builds a [`ReportBundle`] out of the
//! running server's own data planes — one [`ReportSection`] per bundle
//! section, registered into `cauce_core::report`'s process-global
//! registry at collect time.
//!
//! Sections read the same planes their HTTP counterparts render:
//! `config` is `Config::display_tree` (never the raw tree), `stats` is
//! `Store::stats` + `StatsSnapshot::merge_metrics` exactly like
//! `/api/stats`, `engines` is `engine_views` like `/api/engines`,
//! `audit_tail` is the newest 200 `audit` rows, `errors_tail` is the
//! warn/error tail of the JSONL logs parsed as `trace::LogRecord`,
//! `storage` is store/disk counters, `eval_latest` is the newest
//! `evals/results` report. `cauce` carries process metadata.
//!
//! Registration is per-collect and idempotent (the registry dedupes by
//! name), serialized under a lock so concurrent exports — or tests with
//! different `AppState`s — cannot interleave a foreign state's sections.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use cauce_core::report::{self, ReportSection};
use cauce_core::{AuditFilter, RedactionProfile, ReportBundle, ReportCtx, ReportError, evals};
use chrono::Utc;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::app::{AppState, feature_enabled};
use crate::handlers::engine_views;
use crate::observability::trace;

/// Cap on `audit_tail` rows (matches `/api/audit`'s default page).
const AUDIT_TAIL: u32 = 200;
/// Cap on `errors_tail` records.
const ERRORS_TAIL: usize = 100;
/// Compile-time feature names, mirroring [`feature_enabled`]'s match.
const FEATURES: &[&str] = &["ui", "mcp", "ai", "archive", "semantic", "postgres", "otlp"];

/// Serializes register+collect: the built-in sections capture `state`,
/// so a second `collect` must not leak a foreign state's sections into a
/// concurrent export — including tests sharing one registry.
static COLLECT_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Assemble a [`ReportBundle`] for `state`: register the built-in
/// sections (idempotent), collect under the bundle's profile rules, done.
/// `days` is the stats window (7 for the default export);
/// `include_queries` selects `verbose` for this export only — it is a
/// per-export flag and is never persisted.
pub async fn collect(state: &AppState, days: u32, include_queries: bool) -> ReportBundle {
    let _serial = COLLECT_LOCK.lock().await;
    register_builtin_sections(state);
    let ctx = ReportCtx {
        days,
        logs_dir: state.with_config(|c| c.logs_dir()),
        profile: if include_queries {
            RedactionProfile::Verbose
        } else {
            RedactionProfile::Safe
        },
    };
    report::collect(&ctx).await
}

/// Register the v1 section set, each capturing a clone of `state`.
/// Re-registration replaces same-named entries, so repeated `collect`
/// calls against different states stay correct.
fn register_builtin_sections(state: &AppState) {
    for section in [
        Arc::new(CauceSection(state.clone())) as Arc<dyn ReportSection>,
        Arc::new(ConfigSection(state.clone())),
        Arc::new(StatsSection(state.clone())),
        Arc::new(EnginesSection(state.clone())),
        Arc::new(AuditSection(state.clone())),
        Arc::new(ErrorsSection),
        Arc::new(StorageSection(state.clone())),
        Arc::new(EvalSection),
    ] {
        report::register_section(section);
    }
}

/// `config` view helper shared by `cauce`/`storage` sections.
fn with_config<R>(state: &AppState, f: impl FnOnce(&cauce_core::Config) -> R) -> R {
    state.with_config(|c| f(c))
}

/// Log files under `dir`, newest first; a missing directory reads as an
/// empty list (first-run exports legitimately have no logs yet).
fn log_file_list(dir: &Path) -> io::Result<Vec<PathBuf>> {
    match trace::log_files(dir) {
        Ok(files) => Ok(files),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// The `YYYY-MM-DD` a `cauce-<date>.jsonl`/`cauce.<date>.jsonl` file name
/// carries; `None` for undated names (which stay inside every window).
fn log_file_date(path: &Path) -> Option<chrono::NaiveDate> {
    let stem = path.file_stem()?.to_str()?;
    let date = stem
        .strip_prefix("cauce-")
        .or_else(|| stem.strip_prefix("cauce."))?;
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

/// Whether `path` falls inside the `days` window: dated files older
/// than the window are skipped, undated names are kept.
fn within_days(path: &Path, days: u32) -> bool {
    let cutoff = Utc::now().date_naive() - chrono::Duration::days(days as i64);
    log_file_date(path).is_none_or(|d| d >= cutoff)
}

/// `cauce`: process metadata — build version, compiled features, bind
/// address, uptime.
struct CauceSection(AppState);

#[async_trait]
impl ReportSection for CauceSection {
    fn name(&self) -> &'static str {
        "cauce"
    }
    async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
        let bind = with_config(&self.0, |c| format!("{}:{}", c.server.host, c.server.port));
        Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "features": FEATURES.iter().filter(|f| feature_enabled(f)).collect::<Vec<_>>(),
            "bind": bind,
            "uptime_s": self.0.uptime().as_secs(),
        }))
    }
}

/// `config`: `Config::display_tree` — the same redacted projection
/// `GET /api/config` serializes; the raw tree never crosses this seam.
struct ConfigSection(AppState);

#[async_trait]
impl ReportSection for ConfigSection {
    fn name(&self) -> &'static str {
        "config"
    }
    async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
        let tree = with_config(&self.0, |c| c.display_tree())?;
        Ok(serde_json::to_value(tree)?)
    }
}

/// `stats`: `Store::stats(days)` + `merge_metrics` — the `/api/stats`
/// plane verbatim (query-bearing fields fold to `query_hash` under
/// `safe`).
struct StatsSection(AppState);

#[async_trait]
impl ReportSection for StatsSection {
    fn name(&self) -> &'static str {
        "stats"
    }
    async fn collect(&self, ctx: &ReportCtx) -> Result<Value, ReportError> {
        let mut snap = self.0.store().stats(ctx.days).await?;
        snap.merge_metrics();
        Ok(serde_json::to_value(&snap)?)
    }
}

/// `engines`: `engine_views` — the `/api/engines` plane verbatim.
struct EnginesSection(AppState);

#[async_trait]
impl ReportSection for EnginesSection {
    fn name(&self) -> &'static str {
        "engines"
    }
    async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
        let views = engine_views(&self.0)
            .await
            .map_err(|e| ReportError::Other(format!("{e:?}")))?;
        Ok(serde_json::to_value(&views)?)
    }
}

/// `audit_tail`: newest [`AUDIT_TAIL`] `audit` rows inside the `days`
/// window (`details` query leaves stripped and free-form actors dropped
/// under `safe`).
struct AuditSection(AppState);

#[async_trait]
impl ReportSection for AuditSection {
    fn name(&self) -> &'static str {
        "audit_tail"
    }
    async fn collect(&self, ctx: &ReportCtx) -> Result<Value, ReportError> {
        let rows = self
            .0
            .store()
            .list_audit(&AuditFilter {
                since: Some(Utc::now() - chrono::Duration::days(ctx.days as i64)),
                limit: AUDIT_TAIL,
                ..AuditFilter::default()
            })
            .await?;
        Ok(serde_json::to_value(&rows)?)
    }
}

/// `errors_tail`: newest [`ERRORS_TAIL`] warn/error `LogRecord`s across
/// the JSONL files inside the `days` window, `ts` descending (span
/// `fields` query leaves stripped and URL query components dropped under
/// `safe`).
struct ErrorsSection;

#[async_trait]
impl ReportSection for ErrorsSection {
    fn name(&self) -> &'static str {
        "errors_tail"
    }
    async fn collect(&self, ctx: &ReportCtx) -> Result<Value, ReportError> {
        let mut records = Vec::new();
        for path in log_file_list(&ctx.logs_dir)? {
            if !within_days(&path, ctx.days) {
                continue;
            }
            for line in std::fs::read_to_string(&path)?.lines() {
                let Ok(record) = serde_json::from_str::<trace::LogRecord>(line) else {
                    continue;
                };
                if matches!(
                    record.level.to_ascii_lowercase().as_str(),
                    "warn" | "warning" | "error"
                ) {
                    records.push(record);
                }
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.ts));
        records.truncate(ERRORS_TAIL);
        Ok(serde_json::to_value(&records)?)
    }
}

/// `storage`: store/disk counters — db bytes, live cache rows, log
/// retention and the rotated file list.
struct StorageSection(AppState);

#[async_trait]
impl ReportSection for StorageSection {
    fn name(&self) -> &'static str {
        "storage"
    }
    async fn collect(&self, ctx: &ReportCtx) -> Result<Value, ReportError> {
        let snap = self.0.store().stats(ctx.days).await?;
        let retention_days = with_config(&self.0, |c| c.logs.retention_days);
        // `logs_files` lists the files inside the export window, matching
        // what `errors_tail` read.
        let logs_files: Vec<String> = log_file_list(&ctx.logs_dir)?
            .iter()
            .filter(|p| within_days(p, ctx.days))
            .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
            .collect();
        Ok(json!({
            "db_bytes": snap.cache_db_bytes,
            "cache_entries": snap.cache_entries,
            "retention_days": retention_days,
            "logs_files": logs_files,
        }))
    }
}

/// `eval_latest`: the newest `evals/results` report (`Value::Null` when
/// no run exists yet).
struct EvalSection;

#[async_trait]
impl ReportSection for EvalSection {
    fn name(&self) -> &'static str {
        "eval_latest"
    }
    async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
        match evals::latest_report(&evals::results_dir()) {
            Ok(report) => Ok(serde_json::to_value(&report)?),
            Err(e) => Err(ReportError::Other(e.to_string())),
        }
    }
}
