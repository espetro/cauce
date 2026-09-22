//! Process metrics (W1-09, parent plan 6.1): an owned in-process registry
//! of counters, gauges and fixed-bucket histograms that renders Prometheus
//! text for `GET /metrics` and feeds the rolling aggregates behind
//! `/api/stats`'s `engines[]` and `admission` sections.
//!
//! The registry is process-global: [`Metrics`] is a cheap unit handle and
//! every `record_*` writes straight into it, so a pipeline or engine built
//! before `oxe-server` starts serving still lands its samples. There is no
//! external metrics SDK in this crate; OTLP export of traces stays opt-in
//! behind `oxe-server`'s non-default `otlp` cargo feature.
//!
//! Alongside the exposition series the registry keeps rolling percentile
//! windows (per-engine plus admission, SearXNG parity over the current run);
//! [`crate::StatsSnapshot::merge_metrics`] folds those into `/api/stats`
//! while the store keeps serving the day series from `search_log`.
//!
//! Admission recording (`record_admission_wait` / `record_admission_rejected`
//! / `record_stale_served`) is the hook set W1-07's admission code calls;
//! until it lands the pipeline records a zero wait so the series exists.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use crate::engine::{EngineError, EngineId, Tier};
use crate::request::ClientKind;
use crate::store::{AdmissionStats, BreakerState, PhaseStats};

/// Rolling-window size for per-engine/per-queue latency samples. Bounded so
/// a long-running process does not grow the aggregates without limit; the
/// percentiles describe the recent past (SearXNG computes over the current
/// run too).
const WINDOW_CAP: usize = 512;

/// Bucket bounds (ms) for every `*_ms` histogram. Coarse on purpose: the
/// pull endpoint is a health signal, not a profiling backend.
const MS_BUCKETS: &[f64] = &[5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0];

/// Bucket bounds for `oxe_engine_results` (result counts per call).
const COUNT_BUCKETS: &[f64] = &[0.0, 1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0];

/// `phase` label domain of `oxe_engine_duration_ms`. Engines record the
/// phases they actually perform: the upstream fetch leg as `Http`, result
/// extraction as `Parse` (declarative runtime, exec decode). The pipeline
/// never records this instrument itself — only engine runtimes know the
/// split — so `phase` series exist per engine runtime, not per call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePhase {
    Http,
    Parse,
}

impl EnginePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Parse => "parse",
        }
    }
}

/// `outcome` label value for `oxe_engine_requests_total`: `"ok"`, or the
/// `EngineError` variant in snake_case (`rate_limited`, `blocked`,
/// `timeout`, `parse`, `transport`, `no_results`).
pub fn engine_error_label(err: &EngineError) -> &'static str {
    match err {
        EngineError::RateLimited => "rate_limited",
        EngineError::Blocked => "blocked",
        EngineError::Timeout => "timeout",
        EngineError::Parse(_) => "parse",
        EngineError::Transport(_) => "transport",
        EngineError::NoResults => "no_results",
    }
}

/// `engine` label value used by [`crate::Store`] health/state gauges when a
/// breaker state needs a number: `Closed = 0`, `HalfOpen = 1`, `Open = 2`.
pub fn breaker_state_code(state: BreakerState) -> u64 {
    match state {
        BreakerState::Closed => 0,
        BreakerState::HalfOpen => 1,
        BreakerState::Open => 2,
    }
}

fn ms_f64(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn ms_u32(d: Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

// ---------------------------------------------------------------------------
// Registry storage
// ---------------------------------------------------------------------------

/// One label set, kept sorted so `BTreeMap` keys are canonical.
type Labels = Vec<(String, String)>;

fn labels(pairs: &[(&str, String)]) -> Labels {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// Fixed-bucket histogram: per-bucket counts plus running sum/count. The
/// last bucket is the implicit `+Inf` overflow; Prometheus's cumulative
/// `le` series are produced at render time.
struct Hist {
    bounds: &'static [f64],
    buckets: Vec<u64>,
    sum: f64,
    count: u64,
}

impl Hist {
    fn new(bounds: &'static [f64]) -> Self {
        Self {
            bounds,
            buckets: vec![0; bounds.len() + 1],
            sum: 0.0,
            count: 0,
        }
    }

    fn observe(&mut self, v: f64) {
        self.sum += v;
        self.count += 1;
        let idx = self.bounds.partition_point(|b| v > *b);
        self.buckets[idx] += 1;
    }
}

#[derive(Default)]
struct EngineAgg {
    /// Engine calls completed (any outcome).
    requests: u64,
    /// Calls that returned results or `NoResults` (an answer, not a failure).
    ok: u64,
    /// Total results returned across calls.
    result_count: u64,
    /// Whole-call latency as seen by the pipeline.
    total_ms: VecDeque<u32>,
    http_ms: VecDeque<u32>,
    parse_ms: VecDeque<u32>,
}

struct RegistryInner {
    // --- /api/stats rolling aggregates -------------------------------------
    engines: HashMap<EngineId, EngineAgg>,
    admission_waits: VecDeque<u32>,
    admission_rejected: u64,
    rejected_by_reason: BTreeMap<String, u64>,
    deadline_hits: u64,
    stale_served: u64,
    // --- exposition series --------------------------------------------------
    /// `oxe_search_requests_total{client,source,tier}`.
    search_requests: BTreeMap<Labels, u64>,
    /// `oxe_search_duration_ms{source}`.
    search_duration: BTreeMap<Labels, Hist>,
    /// `oxe_ttfr_ms`.
    ttfr: Hist,
    /// `oxe_engine_requests_total{engine,outcome}`.
    engine_requests: BTreeMap<Labels, u64>,
    /// `oxe_engine_duration_ms{engine,phase}`.
    engine_duration: BTreeMap<Labels, Hist>,
    /// `oxe_engine_results{engine}`.
    engine_results: BTreeMap<Labels, Hist>,
    /// `oxe_engine_breaker_state{engine}` — last write wins (gauge).
    breaker_state: BTreeMap<Labels, u64>,
    /// `oxe_admission_wait_ms`.
    admission_wait: Hist,
    /// `oxe_admission_rejected_total{reason}`.
    admission_rejected_series: BTreeMap<Labels, u64>,
}

impl Default for RegistryInner {
    fn default() -> Self {
        Self {
            engines: HashMap::new(),
            admission_waits: VecDeque::new(),
            admission_rejected: 0,
            rejected_by_reason: BTreeMap::new(),
            deadline_hits: 0,
            stale_served: 0,
            search_requests: BTreeMap::new(),
            search_duration: BTreeMap::new(),
            ttfr: Hist::new(MS_BUCKETS),
            engine_requests: BTreeMap::new(),
            engine_duration: BTreeMap::new(),
            engine_results: BTreeMap::new(),
            breaker_state: BTreeMap::new(),
            admission_wait: Hist::new(MS_BUCKETS),
            admission_rejected_series: BTreeMap::new(),
        }
    }
}

static REGISTRY: LazyLock<Mutex<RegistryInner>> =
    LazyLock::new(|| Mutex::new(RegistryInner::default()));

fn registry() -> MutexGuard<'static, RegistryInner> {
    // A poisoned lock only means a recorder panicked mid-update; the
    // aggregates are still consistent enough to keep serving.
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Backing cell of the `oxe_cache_entries` gauge. `oxe-server` refreshes it
/// from the store before each scrape (observable-style, without the OTel
/// callback machinery).
static CACHE_ENTRIES: AtomicU64 = AtomicU64::new(0);

/// Set the live `cache_entries` row count reported by
/// `oxe_cache_entries`. Called by `oxe-server`'s `MetricsHandle`.
pub fn set_cache_entries(n: u64) {
    CACHE_ENTRIES.store(n, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// /api/stats views
// ---------------------------------------------------------------------------

/// Nearest-rank percentile of an unsorted sample list; zeros when empty.
fn percentiles(window: &VecDeque<u32>) -> PhaseStats {
    if window.is_empty() {
        return PhaseStats::default();
    }
    let mut sorted: Vec<u32> = window.iter().copied().collect();
    sorted.sort_unstable();
    let n = sorted.len() as u64;
    let pick = |p: u64| sorted[((n * p).div_ceil(100).max(1) - 1) as usize];
    PhaseStats {
        median_ms: pick(50),
        p80_ms: pick(80),
        p95_ms: pick(95),
    }
}

fn push(window: &mut VecDeque<u32>, ms: u32) {
    if window.len() == WINDOW_CAP {
        window.pop_front();
    }
    window.push_back(ms);
}

/// One engine's in-process aggregates, merged into `/api/stats`'s
/// `engines[]` rows by [`crate::StatsSnapshot::merge_metrics`].
#[derive(Debug, Clone)]
pub struct EngineMetricStats {
    pub engine: EngineId,
    pub requests: u64,
    pub ok: u64,
    pub result_count: u64,
    /// `ok / requests * 100` (SearXNG reliability; 0.0 with no requests).
    pub reliability_pct: f64,
    /// Whole-call percentiles (pipeline-observed latency).
    pub total: PhaseStats,
    pub http: PhaseStats,
    pub parse: PhaseStats,
}

/// Per-engine aggregates for `/api/stats`, sorted by engine id.
pub fn engine_stats() -> Vec<EngineMetricStats> {
    let reg = registry();
    let mut out: Vec<EngineMetricStats> = reg
        .engines
        .iter()
        .map(|(engine, agg)| EngineMetricStats {
            engine: engine.clone(),
            requests: agg.requests,
            ok: agg.ok,
            result_count: agg.result_count,
            reliability_pct: if agg.requests == 0 {
                0.0
            } else {
                agg.ok as f64 / agg.requests as f64 * 100.0
            },
            total: percentiles(&agg.total_ms),
            http: percentiles(&agg.http_ms),
            parse: percentiles(&agg.parse_ms),
        })
        .collect();
    out.sort_by(|a, b| a.engine.cmp(&b.engine));
    out
}

/// Admission/queue aggregates for `/api/stats`.
pub fn admission_stats() -> AdmissionStats {
    let reg = registry();
    let waits = percentiles(&reg.admission_waits);
    AdmissionStats {
        waits: reg.admission_waits.len() as u64,
        wait_median_ms: waits.median_ms,
        wait_p80_ms: waits.p80_ms,
        wait_p95_ms: waits.p95_ms,
        rejected: reg.admission_rejected,
        rejected_by_reason: reg.rejected_by_reason.clone(),
        deadline_hits: reg.deadline_hits,
        stale_served: reg.stale_served,
    }
}

// ---------------------------------------------------------------------------
// Prometheus text render
// ---------------------------------------------------------------------------

/// Escape a label value per the exposition format (`\`, `"`, newline).
fn write_labels(out: &mut String, ls: &Labels) {
    if ls.is_empty() {
        return;
    }
    out.push('{');
    for (i, (k, v)) in ls.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{k}=\"");
        for c in v.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push('}');
}

fn render_counter(out: &mut String, name: &str, help: &str, series: &BTreeMap<Labels, u64>) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    if series.is_empty() {
        // Keep the family present before its first real event: the settled
        // instrument list must appear after a single search.
        let _ = writeln!(out, "{name} 0");
        return;
    }
    for (ls, v) in series {
        out.push_str(name);
        write_labels(out, ls);
        let _ = writeln!(out, " {v}");
    }
}

fn render_gauge(out: &mut String, name: &str, help: &str, series: &BTreeMap<Labels, u64>) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    if series.is_empty() {
        let _ = writeln!(out, "{name} 0");
        return;
    }
    for (ls, v) in series {
        out.push_str(name);
        write_labels(out, ls);
        let _ = writeln!(out, " {v}");
    }
}

/// Emit one histogram series: cumulative `_bucket{le}` lines plus `_sum`
/// and `_count`.
fn render_hist_series(out: &mut String, name: &str, ls: &Labels, h: &Hist) {
    let mut cumulative = 0u64;
    for (i, bound) in h.bounds.iter().enumerate() {
        cumulative += h.buckets[i];
        let mut ls = ls.clone();
        ls.push(("le".to_string(), format!("{bound}")));
        out.push_str(name);
        out.push_str("_bucket");
        write_labels(out, &ls);
        let _ = writeln!(out, " {cumulative}");
    }
    let mut inf = ls.clone();
    inf.push(("le".to_string(), "+Inf".to_string()));
    out.push_str(name);
    out.push_str("_bucket");
    write_labels(out, &inf);
    let _ = writeln!(out, " {}", h.count);
    out.push_str(name);
    out.push_str("_sum");
    write_labels(out, ls);
    let _ = writeln!(out, " {}", h.sum);
    out.push_str(name);
    out.push_str("_count");
    write_labels(out, ls);
    let _ = writeln!(out, " {}", h.count);
}

fn render_hist_header(out: &mut String, name: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} histogram");
}

fn render_hist_empty(out: &mut String, name: &str) {
    let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} 0");
    let _ = writeln!(out, "{name}_sum 0");
    let _ = writeln!(out, "{name}_count 0");
}

fn render_hist_map(out: &mut String, name: &str, help: &str, series: &BTreeMap<Labels, Hist>) {
    render_hist_header(out, name, help);
    if series.is_empty() {
        // Keep the family present before its first observation.
        render_hist_empty(out, name);
        return;
    }
    for (ls, h) in series {
        render_hist_series(out, name, ls, h);
    }
}

/// Label-free histogram (`oxe_ttfr_ms`, `oxe_admission_wait_ms`).
fn render_hist_one(out: &mut String, name: &str, help: &str, h: &Hist) {
    render_hist_header(out, name, help);
    if h.count == 0 {
        render_hist_empty(out, name);
        return;
    }
    render_hist_series(out, name, &Labels::new(), h);
}

/// Prometheus text exposition of the whole registry (the body of
/// `GET /metrics`). Infallible: rendering never fails and an empty registry
/// still produces a parseable document with every settled family present.
pub fn render_prometheus() -> String {
    let reg = registry();
    let mut out = String::with_capacity(4096);

    render_counter(
        &mut out,
        "oxe_search_requests_total",
        "Search requests",
        &reg.search_requests,
    );
    render_hist_map(
        &mut out,
        "oxe_search_duration_ms",
        "Search request latency",
        &reg.search_duration,
    );
    render_hist_one(&mut out, "oxe_ttfr_ms", "Time to first engine result", &reg.ttfr);
    render_counter(
        &mut out,
        "oxe_engine_requests_total",
        "Engine calls",
        &reg.engine_requests,
    );
    render_hist_map(
        &mut out,
        "oxe_engine_duration_ms",
        "Engine phase duration",
        &reg.engine_duration,
    );
    render_hist_map(
        &mut out,
        "oxe_engine_results",
        "Results returned per engine call",
        &reg.engine_results,
    );
    render_gauge(
        &mut out,
        "oxe_engine_breaker_state",
        "Circuit breaker state (0 closed, 1 half-open, 2 open)",
        &reg.breaker_state,
    );
    let _ = writeln!(
        out,
        "# HELP oxe_cache_entries Live (unexpired) cache_entries rows"
    );
    let _ = writeln!(out, "# TYPE oxe_cache_entries gauge");
    let _ = writeln!(
        out,
        "oxe_cache_entries {}",
        CACHE_ENTRIES.load(Ordering::Relaxed)
    );
    render_hist_one(
        &mut out,
        "oxe_admission_wait_ms",
        "Time spent queued by admission control",
        &reg.admission_wait,
    );
    render_counter(
        &mut out,
        "oxe_admission_rejected_total",
        "Requests rejected by admission control",
        &reg.admission_rejected_series,
    );
    let _ = writeln!(
        out,
        "# HELP oxe_deadline_hit_total Searches cut by the hard deadline"
    );
    let _ = writeln!(out, "# TYPE oxe_deadline_hit_total counter");
    let _ = writeln!(out, "oxe_deadline_hit_total {}", reg.deadline_hits);
    let _ = writeln!(
        out,
        "# HELP oxe_stale_served_total Responses served from an expired cache row"
    );
    let _ = writeln!(out, "# TYPE oxe_stale_served_total counter");
    let _ = writeln!(out, "oxe_stale_served_total {}", reg.stale_served);
    out
}

// ---------------------------------------------------------------------------
// Metrics handle (record API)
// ---------------------------------------------------------------------------

/// Cloneable handle over the settled W1-09 instrument set. The registry is
/// process-global, so the handle carries no state: `Metrics::default()` and
/// a handle passed through `SearchPipeline::with_metrics` record into the
/// same series.
#[derive(Clone, Copy, Default)]
pub struct Metrics;

impl Metrics {
    /// `oxe_search_requests_total{client,source,tier}` +
    /// `oxe_search_duration_ms{source}`. `source` is `"cache"` or
    /// `"network"`; `tier` is the serving cache tier for hits, `None` on the
    /// network path (labelled `"none"` so the set stays rectangular).
    pub fn record_search(
        &self,
        client: &ClientKind,
        source: &'static str,
        tier: Option<Tier>,
        elapsed: Duration,
    ) {
        let mut reg = registry();
        *reg.search_requests
            .entry(labels(&[
                ("client", client.label()),
                ("source", source.to_string()),
                (
                    "tier",
                    tier.map(|t| t.as_u8().to_string())
                        .unwrap_or_else(|| "none".to_string()),
                ),
            ]))
            .or_insert(0) += 1;
        reg.search_duration
            .entry(labels(&[("source", source.to_string())]))
            .or_insert_with(|| Hist::new(MS_BUCKETS))
            .observe(ms_f64(elapsed));
    }

    /// `oxe_ttfr_ms` — call once per search with the latency of the first
    /// successful engine response.
    pub fn record_ttfr(&self, d: Duration) {
        registry().ttfr.observe(ms_f64(d));
    }

    /// One completed engine call: `oxe_engine_requests_total{engine,outcome}`,
    /// `oxe_engine_results{engine}` when the call produced an answer
    /// (`results = Some(n)`, including `NoResults` as `Some(0)`), and the
    /// registry's whole-call sample for `/api/stats`.
    pub fn record_engine_call(
        &self,
        engine: &EngineId,
        outcome: &'static str,
        latency: Duration,
        results: Option<usize>,
    ) {
        let mut reg = registry();
        let engine_label = ("engine", engine.as_str().to_string());
        *reg.engine_requests
            .entry(labels(&[
                engine_label.clone(),
                ("outcome", outcome.to_string()),
            ]))
            .or_insert(0) += 1;
        if let Some(n) = results {
            reg.engine_results
                .entry(labels(&[engine_label]))
                .or_insert_with(|| Hist::new(COUNT_BUCKETS))
                .observe(n as f64);
        }
        let agg = reg.engines.entry(engine.clone()).or_default();
        agg.requests += 1;
        if matches!(outcome, "ok" | "no_results") {
            agg.ok += 1;
        }
        agg.result_count += results.unwrap_or(0) as u64;
        push(&mut agg.total_ms, ms_u32(latency));
    }

    /// `oxe_engine_duration_ms{engine,phase}` — recorded by engine runtimes
    /// for the phases they perform (`Http` fetch leg, `Parse` extraction).
    pub fn record_engine_phase(&self, engine: &EngineId, phase: EnginePhase, d: Duration) {
        let mut reg = registry();
        reg.engine_duration
            .entry(labels(&[
                ("engine", engine.as_str().to_string()),
                ("phase", phase.as_str().to_string()),
            ]))
            .or_insert_with(|| Hist::new(MS_BUCKETS))
            .observe(ms_f64(d));
        let agg = reg.engines.entry(engine.clone()).or_default();
        let window = match phase {
            EnginePhase::Http => &mut agg.http_ms,
            EnginePhase::Parse => &mut agg.parse_ms,
        };
        push(window, ms_u32(d));
    }

    /// `oxe_engine_breaker_state{engine}` — current breaker state as
    /// 0/1/2 (closed/half-open/open). Pre-W1-06 the pipeline reports
    /// `Closed` for every engine it runs.
    pub fn record_breaker_state(&self, engine: &EngineId, state: BreakerState) {
        registry().breaker_state.insert(
            labels(&[("engine", engine.as_str().to_string())]),
            breaker_state_code(state),
        );
    }

    /// `oxe_admission_wait_ms` — queue wait for the request. Pre-W1-07
    /// admission is pass-through, so the pipeline records `Duration::ZERO`.
    pub fn record_admission_wait(&self, d: Duration) {
        let mut reg = registry();
        reg.admission_wait.observe(ms_f64(d));
        push(&mut reg.admission_waits, ms_u32(d));
    }

    /// `oxe_admission_rejected_total{reason}` — W1-07 calls this on queue
    /// overflow (`"queue_full"`, `"wait_timeout"`).
    pub fn record_admission_rejected(&self, reason: &'static str) {
        let mut reg = registry();
        *reg.admission_rejected_series
            .entry(labels(&[("reason", reason.to_string())]))
            .or_insert(0) += 1;
        reg.admission_rejected += 1;
        *reg.rejected_by_reason
            .entry(reason.to_string())
            .or_insert(0) += 1;
    }

    /// `oxe_deadline_hit_total` — one per search whose hard deadline
    /// cancelled in-flight engine calls.
    pub fn record_deadline_hit(&self) {
        registry().deadline_hits += 1;
    }

    /// `oxe_stale_served_total` — one per response served from an expired
    /// cache row (the W1-07 overflow path).
    pub fn record_stale_served(&self) {
        registry().stale_served += 1;
    }
}
