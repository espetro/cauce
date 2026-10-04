//! Report bundles (#238, from the #235 design comment): the seam between
//! cauce's existing data planes and a shareable export — a
//! `cauce-report-<ts>.json` document assembled from named sections under a
//! `safe`/`verbose` [`RedactionProfile`].
//!
//! Three pieces:
//!
//! - [`ReportBundle`]: the wire schema (`v = 1`) — fixed envelope fields
//!   plus one flattened entry per section payload (`cauce`, `config`,
//!   `stats`, `engines`, `audit_tail`, `errors_tail`, `storage`,
//!   `eval_latest` in the v1 collector set).
//! - The section/sink seam: [`ReportSection`] produces a section payload
//!   from a [`ReportCtx`]; [`ReportSink`] receives emitted
//!   [`ReportEvent`]s. Both register into a process-global registry shaped
//!   like `metrics.rs`'s `REGISTRY` — producers and consumers never see
//!   each other, so the #230 agent crate can register its own
//!   section/sink later without touching the collector.
//! - [`redact`]: the `safe`-profile pass applied to every bundle at
//!   collect time (#241).
//!
//! Axum-free, same boundary rule as the agent crate: this module sees
//! only `serde_json::Value` payloads and plain data — no HTTP types.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod redact;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::store::BreakerState;
use crate::{ConfigError, EngineId, StoreError};

pub use redact::RedactionProfile;

/// Bundle schema version (`v`).
pub const SCHEMA_VERSION: u32 = 1;

/// Envelope field names a section payload may not claim — the schema's
/// fixed keys stay fixed.
const RESERVED_KEYS: &[&str] = &["v", "profile", "generated_at", "notes"];

/// Errors a section may report. A failed section is omitted from the
/// bundle (with a `warn!`), never fatal to the export.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// Cross-cutting collection inputs. Everything else a section needs —
/// the store, the live config, the pipeline — it captures at registration
/// time, keeping this struct free of `cauce-server` shapes.
#[derive(Debug)]
pub struct ReportCtx {
    /// The export window in days (7 for the default export): the
    /// `stats`-family sections' aggregate window and the cutoff for
    /// `audit_tail` rows and the log files `errors_tail`/`storage` read.
    pub days: u32,
    /// `<data_dir>/logs`, for the `errors_tail`/`storage` sections.
    pub logs_dir: PathBuf,
    /// The profile the bundle is redacted under (`--include-queries`
    /// selects `verbose`, per export only — never persisted).
    pub profile: RedactionProfile,
}

/// A bundle section anyone can register: `name` is the payload's key,
/// `collect` produces the payload.
#[async_trait]
pub trait ReportSection: Send + Sync {
    /// The section's key in the bundle — `"stats"`, `"engines"`,
    /// `"audit_tail"`, `"agent"`, ... [`RESERVED_KEYS`] panic at
    /// registration.
    fn name(&self) -> &'static str;
    /// Produce the section payload. The export applies the
    /// [`RedactionProfile`] afterwards; sections should already source
    /// redacted-safe values where a plane offers them (e.g.
    /// `Config::display_tree`, never the raw tree).
    async fn collect(&self, ctx: &ReportCtx) -> Result<Value, ReportError>;
}

/// Sinks receive emitted events; exporters are sinks with egress.
pub trait ReportSink: Send + Sync {
    /// Identity for registry de-dup (`"jsonl"`, `"otlp"`, ...).
    fn name(&self) -> &'static str;
    /// Observe one event. Called synchronously on the producer's path —
    /// keep it cheap or hand off.
    fn record(&self, ev: &ReportEvent);
}

/// A typed report event — non-sensitive fields *by construction*, the
/// same trick `AuditRow` already plays: nothing on this enum can carry a
/// query string, a prompt or a secret, because no variant has a field
/// for them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportEvent {
    /// `observability::audit()` dual-writes a row — the third fan-out
    /// after the JSONL event and the `audit` table insert.
    Audit {
        actor: String,
        action: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<Uuid>,
    },
    /// An engine call finished with an error. `error` is the
    /// `metrics::engine_error_label` class, never the message text —
    /// transport errors can embed upstream URLs.
    EngineFailure { engine: EngineId, error: String },
    /// A breaker changed state.
    BreakerTransition {
        engine: EngineId,
        from: BreakerState,
        to: BreakerState,
    },
    /// A search completed (cache hit or network). No query field: the
    /// hash joins live on the search-log and audit planes.
    SearchOutcome {
        /// The metrics `outcome` label (`ok` / `error` / `rejected`).
        outcome: String,
        latency_ms: u32,
        result_count: u32,
    },
    /// `PUT /api/config` applied; `paths` are dotted keys, never values.
    ConfigChanged { paths: Vec<String> },
    /// An AI provider call completed.
    AiCall {
        model: String,
        outcome: String,
        duration_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        completion_tokens: Option<u32>,
    },
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The process-global registries — same shape as `metrics.rs`'s
/// `REGISTRY`: cheap `RwLock`-guarded lists, last-registered-wins on a
/// name collision, a poisoned lock still serves (`into_inner`).
static SECTIONS: LazyLock<RwLock<Vec<Arc<dyn ReportSection>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));
static SINKS: LazyLock<RwLock<Vec<Arc<dyn ReportSink>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Register `section`, replacing any same-named one. Registration order
/// is preserved so a bundle's sections render in a stable order.
///
/// Panics when `section.name()` collides with an envelope field
/// (`v`, `profile`, `generated_at`, `notes`).
pub fn register_section(section: Arc<dyn ReportSection>) {
    assert!(
        !RESERVED_KEYS.contains(&section.name()),
        "report section name {:?} collides with an envelope field",
        section.name()
    );
    let mut guard = SECTIONS.write().unwrap_or_else(|e| e.into_inner());
    guard.retain(|s| s.name() != section.name());
    guard.push(section);
}

/// Register `sink`, replacing any same-named one.
pub fn register_sink(sink: Arc<dyn ReportSink>) {
    let mut guard = SINKS.write().unwrap_or_else(|e| e.into_inner());
    guard.retain(|s| s.name() != sink.name());
    guard.push(sink);
}

/// Registered sections in registration order.
pub fn sections() -> Vec<Arc<dyn ReportSection>> {
    SECTIONS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn sinks() -> Vec<Arc<dyn ReportSink>> {
    SINKS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Emit `ev` to every registered sink. A panicking sink is logged and
/// skipped — a bad sink must never break its producer (e.g. `audit()`).
pub fn record(ev: &ReportEvent) {
    for sink in sinks() {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.record(ev))).is_err() {
            tracing::warn!(sink = sink.name(), "report sink panicked; event dropped");
        }
    }
}

// ---------------------------------------------------------------------------
// Bundle
// ---------------------------------------------------------------------------

/// The export document (`cauce-report-<ts>.json`, `v = 1`): the fixed
/// envelope plus one flattened entry per registered section, so
/// `cauce`, `config`, `stats`, ... sit beside the envelope keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportBundle {
    /// Schema version — always [`SCHEMA_VERSION`].
    pub v: u32,
    /// The profile this bundle was built (and already redacted) under.
    pub profile: RedactionProfile,
    /// When the export was assembled.
    pub generated_at: DateTime<Utc>,
    /// Owner free-text, never auto-filled.
    #[serde(default)]
    pub notes: String,
    /// Section payloads keyed by `ReportSection::name`, flattened into
    /// the document root.
    #[serde(flatten)]
    pub sections: BTreeMap<String, Value>,
}

impl ReportBundle {
    /// Collect every registered section and run the redaction pass — the
    /// only way a bundle comes into being. A failing section is omitted
    /// with a `warn!`, never fatal to the export.
    pub async fn collect(ctx: &ReportCtx) -> Self {
        let mut sections = BTreeMap::new();
        for section in self::sections() {
            match section.collect(ctx).await {
                Ok(payload) => {
                    sections.insert(section.name().to_string(), payload);
                }
                Err(e) => {
                    tracing::warn!(
                        section = section.name(),
                        error = %e,
                        "report section failed; omitted from bundle"
                    );
                }
            }
        }
        let mut bundle = Self {
            v: SCHEMA_VERSION,
            profile: ctx.profile,
            generated_at: Utc::now(),
            notes: String::new(),
            sections,
        };
        redact::apply(&mut bundle);
        bundle
    }

    /// The export file body (pretty JSON).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ReportBundle serializes")
    }
}

/// Collect every registered section into a redacted [`ReportBundle`].
pub async fn collect(ctx: &ReportCtx) -> ReportBundle {
    ReportBundle::collect(ctx).await
}

// ---------------------------------------------------------------------------
// Share flow (#240/#242)
// ---------------------------------------------------------------------------

/// `issues/new` endpoint the share flow points at.
const ISSUES_NEW_URL: &str = "https://github.com/espetro/cauce/issues/new";

/// The export file name for `ts` — `cauce-report-20260929T093000Z.json`.
/// `GET /api/report` serves it in `Content-Disposition`; `cauce report`
/// writes it under cwd when `--out` is not given.
pub fn filename(ts: &DateTime<Utc>) -> String {
    format!("cauce-report-{}.json", ts.format("%Y%m%dT%H%M%SZ"))
}

/// Title + body for the prefilled GitHub issue — shared by
/// [`issue_url`] (the `X-Report-Issue-Url` response header) and `cauce
/// report --gh`, which feeds the same text to `gh issue create`.
#[derive(Debug, Clone)]
pub struct IssueDraft {
    pub title: String,
    /// The issue body: a one-paragraph bundle summary plus the attach
    /// instructions — URL parameters carry no file payload, so the
    /// bundle itself still has to be dragged in or pasted.
    pub body: String,
}

/// The issue text a bundle shares. High-signal fields (`cauce`,
/// `stats.searches`) are read opportunistically — a bundle missing them
/// still produces a usable draft.
pub fn issue_draft(bundle: &ReportBundle) -> IssueDraft {
    let generated = bundle.generated_at.format("%Y-%m-%d %H:%M UTC");
    let title = format!("cauce report {generated}");
    let mut body = format!(
        "Report bundle v{} ({} profile), generated {}.",
        bundle.v,
        bundle.profile.as_str(),
        bundle.generated_at.to_rfc3339(),
    );
    if let Some(cauce) = bundle.sections.get("cauce") {
        let version = cauce.get("version").and_then(Value::as_str).unwrap_or("?");
        let bind = cauce.get("bind").and_then(Value::as_str).unwrap_or("?");
        let uptime = cauce.get("uptime_s").and_then(Value::as_u64).unwrap_or(0);
        body.push_str(&format!("\n\ncauce {version} on {bind}, uptime {uptime}s."));
    }
    if let Some(searches) = bundle
        .sections
        .get("stats")
        .and_then(|s| s.get("searches"))
        .and_then(Value::as_u64)
    {
        body.push_str(&format!(" {searches} searches in the window."));
    }
    body.push_str(&format!(
        "\n\nAttach `{}` — download it from `GET /api/report` \
         (the `/settings` Logging section links it) or produce one \
         with `cauce report --out <file>.json`.",
        filename(&bundle.generated_at),
    ));
    IssueDraft { title, body }
}

/// The prefilled `issues/new` URL for `bundle`.
pub fn issue_url(bundle: &ReportBundle) -> String {
    let draft = issue_draft(bundle);
    let mut url = url::Url::parse(ISSUES_NEW_URL).expect("issues/new URL parses");
    url.query_pairs_mut()
        .append_pair("title", &draft.title)
        .append_pair("body", &draft.body);
    url.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Probe {
        name: &'static str,
        payload: Value,
    }

    #[async_trait]
    impl ReportSection for Probe {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
            Ok(self.payload.clone())
        }
    }

    struct Failing;

    #[async_trait]
    impl ReportSection for Failing {
        fn name(&self) -> &'static str {
            "probe_failing"
        }
        async fn collect(&self, _ctx: &ReportCtx) -> Result<Value, ReportError> {
            Err(ReportError::Other("boom".into()))
        }
    }

    struct RecordingSink {
        seen: Mutex<Vec<ReportEvent>>,
    }

    static SEEN_COUNT: AtomicUsize = AtomicUsize::new(0);

    impl ReportSink for RecordingSink {
        fn name(&self) -> &'static str {
            "recording"
        }
        fn record(&self, ev: &ReportEvent) {
            SEEN_COUNT.fetch_add(1, Ordering::Relaxed);
            self.seen.lock().unwrap().push(ev.clone());
        }
    }

    fn ctx() -> ReportCtx {
        ReportCtx {
            days: 7,
            logs_dir: PathBuf::from("/nonexistent"),
            profile: RedactionProfile::Safe,
        }
    }

    /// Serialises registry-mutating tests (the registry is process-global
    /// and the test binary runs cases in parallel threads).
    static REG_LOCK: Mutex<()> = Mutex::new(());

    // REG_LOCK serializes registration+collect in one hold: the lock is a
    // test-only serializer, never re-entered, so holding it across the
    // collect await is exactly the intent.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn collect_assembles_envelope_and_sections() {
        let _g = REG_LOCK.lock().unwrap();
        register_section(Arc::new(Probe {
            name: "probe_a",
            payload: serde_json::json!({"ok": true}),
        }));
        register_section(Arc::new(Failing));
        let bundle = collect(&ctx()).await;
        assert_eq!(bundle.v, SCHEMA_VERSION);
        assert_eq!(bundle.profile, RedactionProfile::Safe);
        assert_eq!(
            bundle.sections.get("probe_a"),
            Some(&serde_json::json!({"ok": true}))
        );
        // A failing section is omitted, not fatal.
        assert!(!bundle.sections.contains_key("probe_failing"));
    }

    #[test]
    fn register_section_dedupes_by_name() {
        let _g = REG_LOCK.lock().unwrap();
        register_section(Arc::new(Probe {
            name: "probe_dedup",
            payload: serde_json::json!(1),
        }));
        register_section(Arc::new(Probe {
            name: "probe_dedup",
            payload: serde_json::json!(2),
        }));
        let registered = sections();
        let probes: Vec<_> = registered
            .iter()
            .filter(|s| s.name() == "probe_dedup")
            .collect();
        assert_eq!(probes.len(), 1);
    }

    #[test]
    #[should_panic(expected = "collides with an envelope field")]
    fn reserved_section_name_panics() {
        register_section(Arc::new(Probe {
            name: "notes",
            payload: Value::Null,
        }));
    }

    #[test]
    fn sinks_receive_emitted_events() {
        let _g = REG_LOCK.lock().unwrap();
        let sink = Arc::new(RecordingSink {
            seen: Mutex::new(Vec::new()),
        });
        register_sink(sink.clone());
        record(&ReportEvent::Audit {
            actor: "ui".into(),
            action: "cache.delete".into(),
            target: "abc123".into(),
            request_id: None,
        });
        let seen = sink.seen.lock().unwrap();
        let Some(ReportEvent::Audit { action, .. }) = seen.last() else {
            panic!("expected an audit event, got {seen:?}");
        };
        assert_eq!(action.as_str(), "cache.delete");
        assert!(SEEN_COUNT.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn bundle_json_roundtrips() {
        let _g = REG_LOCK.lock().unwrap();
        let mut sections = BTreeMap::new();
        sections.insert("cauce".to_string(), serde_json::json!({"version": "0.0.0"}));
        sections.insert("stats".to_string(), serde_json::json!({"searches": 3}));
        let bundle = ReportBundle {
            v: SCHEMA_VERSION,
            profile: RedactionProfile::Safe,
            generated_at: DateTime::parse_from_rfc3339("2026-09-28T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            notes: String::new(),
            sections,
        };
        let text = bundle.to_json();
        let parsed: ReportBundle = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.v, 1);
        assert_eq!(parsed.sections.len(), 2);
        // Flattened sections sit beside the envelope keys on the wire.
        let raw: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(raw["v"], serde_json::json!(1));
        assert_eq!(raw["cauce"]["version"], serde_json::json!("0.0.0"));
        assert!(raw.get("sections").is_none());
    }
}
