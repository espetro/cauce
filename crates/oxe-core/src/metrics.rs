//! Process metrics (W1-09, parent plan 6.1): the OTel instrument set feeding
//! `GET /metrics` and the OTLP exporter, plus the in-memory rolling
//! aggregates backing `/api/stats`'s `engines[]` and `admission` sections.
//!
//! [`Metrics`] is a cheap cloneable handle over the twelve settled
//! instruments. Instrument creation binds to the provider behind the meter:
//! [`Metrics::default()`] stays unbound until the first record call, which
//! resolves `opentelemetry::global::meter("oxe")` — so a pipeline built
//! before the provider is installed still lands its samples once it exists.
//! [`Metrics::new`] binds an explicit meter (the server's `MetricsHandle`,
//! or a test-local provider).
//!
//! The [`StatsRegistry`] is a process-global rolling aggregate (SearXNG
//! parity: per-engine percentile windows plus admission counters) that every
//! `record_*` writes through to, independent of whether an OTel provider is
//! installed. `oxe-server` merges it into `StatsSnapshot`; the store keeps
//! serving the day series from `search_log`.
//!
//! Admission recording (`record_admission_wait` / `record_admission_rejected`
//! / `record_stale_served`) is the hook set W1-07's admission code calls;
//! until it lands the pipeline records a zero wait so the series exists.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};

use crate::engine::{EngineError, EngineId, Tier};
use crate::request::ClientKind;
use crate::store::{AdmissionStats, BreakerState, PhaseStats};

/// Rolling-window size for per-engine/per-queue latency samples. Bounded so
/// a long-running process does not grow the aggregates without limit; the
/// percentiles describe the recent past (SearXNG computes over the current
/// run too).
const WINDOW_CAP: usize = 512;

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
// In-memory rolling aggregates (`/api/stats`)
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

#[derive(Default)]
struct RegistryInner {
    engines: HashMap<EngineId, EngineAgg>,
    admission_waits: VecDeque<u32>,
    admission_rejected: u64,
    rejected_by_reason: BTreeMap<String, u64>,
    deadline_hits: u64,
    stale_served: u64,
}

static REGISTRY: LazyLock<Mutex<RegistryInner>> =
    LazyLock::new(|| Mutex::new(RegistryInner::default()));

fn registry() -> std::sync::MutexGuard<'static, RegistryInner> {
    // A poisoned lock only means a recorder panicked mid-update; the
    // aggregates are still consistent enough to keep serving.
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
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
// OTel instruments
// ---------------------------------------------------------------------------

/// The settled W1-09 instrument set. `oxe_cache_entries` is deliberately not
/// here: it is an observable gauge whose callback needs store access, so the
/// server-side `MetricsHandle` registers it on the provider's meter.
struct Instruments {
    /// `oxe_search_requests_total{client,source,tier}` — every search path.
    search_requests: Counter<u64>,
    /// `oxe_search_duration_ms{source}` — whole-request latency.
    search_duration_ms: Histogram<f64>,
    /// `oxe_ttfr_ms` — time to the first successful engine response.
    ttfr_ms: Histogram<f64>,
    /// `oxe_engine_requests_total{engine,outcome}` — every engine call.
    engine_requests: Counter<u64>,
    /// `oxe_engine_duration_ms{engine,phase}` — per-phase engine timings,
    /// recorded by the engine runtimes (http = fetch leg, parse = extract).
    engine_duration_ms: Histogram<f64>,
    /// `oxe_engine_results{engine}` — results returned per call.
    engine_results: Histogram<u64>,
    /// `oxe_engine_breaker_state{engine}` — 0 closed / 1 half-open / 2 open.
    engine_breaker_state: Gauge<u64>,
    /// `oxe_admission_wait_ms` — time spent in the admission queue.
    admission_wait_ms: Histogram<f64>,
    /// `oxe_admission_rejected_total{reason}` — queue overflows / timeouts.
    admission_rejected: Counter<u64>,
    /// `oxe_deadline_hit_total` — searches that hit the hard deadline.
    deadline_hit: Counter<u64>,
    /// `oxe_stale_served_total` — responses served from an expired row.
    stale_served: Counter<u64>,
}

impl Instruments {
    fn new(meter: &Meter) -> Self {
        let i = Self {
            search_requests: meter
                .u64_counter("oxe_search_requests_total")
                .with_description("Search requests")
                .build(),
            search_duration_ms: meter
                .f64_histogram("oxe_search_duration_ms")
                .with_description("Search request latency")
                .build(),
            ttfr_ms: meter
                .f64_histogram("oxe_ttfr_ms")
                .with_description("Time to first engine result")
                .build(),
            engine_requests: meter
                .u64_counter("oxe_engine_requests_total")
                .with_description("Engine calls")
                .build(),
            engine_duration_ms: meter
                .f64_histogram("oxe_engine_duration_ms")
                .with_description("Engine phase duration")
                .build(),
            engine_results: meter
                .u64_histogram("oxe_engine_results")
                .with_description("Results returned per engine call")
                .build(),
            engine_breaker_state: meter
                .u64_gauge("oxe_engine_breaker_state")
                .with_description("Circuit breaker state (0 closed, 1 half-open, 2 open)")
                .build(),
            admission_wait_ms: meter
                .f64_histogram("oxe_admission_wait_ms")
                .with_description("Time spent queued by admission control")
                .build(),
            admission_rejected: meter
                .u64_counter("oxe_admission_rejected_total")
                .with_description("Requests rejected by admission control")
                .build(),
            deadline_hit: meter
                .u64_counter("oxe_deadline_hit_total")
                .with_description("Searches cut by the hard deadline")
                .build(),
            stale_served: meter
                .u64_counter("oxe_stale_served_total")
                .with_description("Responses served from an expired cache row")
                .build(),
        };
        // Zero-seed the event counters so the series exist before their
        // first real event: Prometheus only exports series that have a data
        // point, and the settled list must appear after a single search.
        i.admission_rejected.add(0, &[]);
        i.deadline_hit.add(0, &[]);
        i.stale_served.add(0, &[]);
        i
    }
}

/// Cloneable handle over the settled instrument set. Clones share one
/// binding: a default (unbound) handle resolves `global::meter("oxe")` on
/// its first record; [`Metrics::new`] binds an explicit meter eagerly.
#[derive(Clone, Default)]
pub struct Metrics {
    instruments: Arc<OnceLock<Instruments>>,
}

impl Metrics {
    /// Bind to `meter` now (server `MetricsHandle`, test-local providers).
    pub fn new(meter: &Meter) -> Self {
        let metrics = Self::default();
        // Fresh OnceLock: `set` cannot fail here.
        let _ = metrics.instruments.set(Instruments::new(meter));
        metrics
    }

    fn instruments(&self) -> &Instruments {
        self.instruments
            .get_or_init(|| Instruments::new(&opentelemetry::global::meter("oxe")))
    }

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
        let i = self.instruments();
        i.search_requests.add(
            1,
            &[
                KeyValue::new("client", client.label()),
                KeyValue::new("source", source),
                KeyValue::new(
                    "tier",
                    tier.map(|t| t.as_u8().to_string())
                        .unwrap_or_else(|| "none".to_string()),
                ),
            ],
        );
        i.search_duration_ms
            .record(ms_f64(elapsed), &[KeyValue::new("source", source)]);
    }

    /// `oxe_ttfr_ms` — call once per search with the latency of the first
    /// successful engine response.
    pub fn record_ttfr(&self, d: Duration) {
        self.instruments().ttfr_ms.record(ms_f64(d), &[]);
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
        let engine_kv = KeyValue::new("engine", engine.as_str().to_string());
        let i = self.instruments();
        i.engine_requests
            .add(1, &[engine_kv.clone(), KeyValue::new("outcome", outcome)]);
        if let Some(n) = results {
            i.engine_results.record(n as u64, &[engine_kv]);
        }
        let mut reg = registry();
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
        self.instruments().engine_duration_ms.record(
            ms_f64(d),
            &[
                KeyValue::new("engine", engine.as_str().to_string()),
                KeyValue::new("phase", phase.as_str()),
            ],
        );
        let mut reg = registry();
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
        self.instruments().engine_breaker_state.record(
            breaker_state_code(state),
            &[KeyValue::new("engine", engine.as_str().to_string())],
        );
    }

    /// `oxe_admission_wait_ms` — queue wait for the request. Pre-W1-07
    /// admission is pass-through, so the pipeline records `Duration::ZERO`.
    pub fn record_admission_wait(&self, d: Duration) {
        self.instruments().admission_wait_ms.record(ms_f64(d), &[]);
        push(&mut registry().admission_waits, ms_u32(d));
    }

    /// `oxe_admission_rejected_total{reason}` — W1-07 calls this on queue
    /// overflow (`"queue_full"`, `"wait_timeout"`).
    pub fn record_admission_rejected(&self, reason: &'static str) {
        self.instruments()
            .admission_rejected
            .add(1, &[KeyValue::new("reason", reason)]);
        let mut reg = registry();
        reg.admission_rejected += 1;
        *reg.rejected_by_reason
            .entry(reason.to_string())
            .or_insert(0) += 1;
    }

    /// `oxe_deadline_hit_total` — one per search whose hard deadline
    /// cancelled in-flight engine calls.
    pub fn record_deadline_hit(&self) {
        self.instruments().deadline_hit.add(1, &[]);
        registry().deadline_hits += 1;
    }

    /// `oxe_stale_served_total` — one per response served from an expired
    /// cache row (the W1-07 overflow path).
    pub fn record_stale_served(&self) {
        self.instruments().stale_served.add(1, &[]);
        registry().stale_served += 1;
    }
}
