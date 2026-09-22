//! Engine health: EWMA latency, consecutive-failure counting and the
//! circuit breaker (parent plan 4.4 step 6; wave-1 settled inputs).
//!
//! [`HealthTracker`] holds one [`EngineHealth`] per engine, consulted by the
//! pipeline before fan-out ([`HealthTracker::admission`]) and updated after
//! every engine call ([`HealthTracker::record_ok`] /
//! [`HealthTracker::record_err`]):
//!
//! - EWMA latency with `alpha = 0.3`, seeded by the first sample.
//! - `RateLimited`/`Blocked` open the breaker for 15 min; 3 consecutive
//!   `Timeout`s open it for 5 min (`HealthPolicy`).
//! - `Open` engines are skipped; once `breaker_until` passes the state
//!   flips to `HalfOpen` and exactly one call is let through as a probe.
//!   A successful probe closes the breaker; a failed one re-opens it.
//! - `NoResults` is *not* a failure: the engine answered (the pipeline
//!   already treats it as a 200-shaped empty response), so it resets the
//!   failure counter like any other answer.
//!
//! State is persisted through `Store::put_health`, debounced to one flush
//! per second (`PERSIST_DEBOUNCE`) for routine EWMA/failure updates, while
//! breaker transitions flush urgently on the next `flush_due` — the
//! `engine_health` row is on disk before the search that tripped the
//! breaker returns, so a restart does not hammer a blocked engine.
//! Breaker transitions are also appended to the `audit` table
//! (`engine.breaker`, parent plan 6.1). [`HealthTracker::load`] restores
//! persisted rows at startup.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::engine::{EngineError, EngineId};
use crate::store::{AuditRow, BreakerState, EngineHealthRow, Store, StoreError};

/// EWMA weight of each new latency sample (settled: `alpha = 0.3`).
pub const EWMA_ALPHA: f64 = 0.3;

/// Minimum interval between routine `put_health` flushes (settled:
/// debounced 1/s). Breaker transitions bypass the debounce: they are rare,
/// audited, and must reach disk before the triggering response returns.
pub const PERSIST_DEBOUNCE: Duration = Duration::from_secs(1);

/// Audit actor for transitions the scheduler itself makes (no inbound
/// request actor exists; `POST /api/engines/{id}/reset` audits with the
/// caller's actor instead).
const SYSTEM_ACTOR: &str = "oxe";

/// Breaker knobs (settled inputs). Kept as a struct so tests can shrink
/// the windows instead of sleeping minutes; production uses
/// `HealthPolicy::default` via [`HealthTracker::new`].
#[derive(Debug, Clone)]
pub struct HealthPolicy {
    /// Open window after `RateLimited` or `Blocked` (15 min).
    pub abuse_window: Duration,
    /// Consecutive `Timeout`s that open the breaker (3).
    pub timeout_threshold: u32,
    /// Open window after `timeout_threshold` consecutive timeouts, and the
    /// re-open window for a failed half-open probe (5 min).
    pub timeout_window: Duration,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            abuse_window: Duration::from_secs(15 * 60),
            timeout_threshold: 3,
            timeout_window: Duration::from_secs(5 * 60),
        }
    }
}

/// In-memory health of one engine; [`EngineHealthRow`] is its persisted
/// projection. `samples`, `timeout_streak` and `probe_in_flight` are
/// runtime-only.
#[derive(Debug, Clone)]
pub struct EngineHealth {
    /// EWMA of observed call latency in ms (`alpha = 0.3`).
    pub ewma_ms: f64,
    /// Consecutive failures of any kind; reset by any answer (`Ok` or
    /// `NoResults`).
    pub failures: u32,
    pub breaker: BreakerState,
    /// `Open` flips to `HalfOpen` at this instant.
    pub breaker_until: Option<DateTime<Utc>>,
    pub last_ok_at: Option<DateTime<Utc>>,
    /// Last `EngineError` rendered for display; kept after recovery.
    pub last_error: Option<String>,
    /// Latency samples seen; the first seeds the EWMA instead of blending
    /// toward zero.
    samples: u64,
    /// Consecutive `Timeout`s specifically — the settled rule is "3
    /// consecutive timeouts", so a `Parse` between two timeouts breaks the
    /// streak. Runtime-only: `engine_health` has no column for it, and a
    /// restart conservatively assumes the streak is broken.
    timeout_streak: u32,
    /// `HalfOpen` single-probe gate: true while a probe call is in flight.
    /// Runtime-only — a restarted process has no probes in flight.
    probe_in_flight: bool,
}

impl Default for EngineHealth {
    fn default() -> Self {
        Self {
            ewma_ms: 0.0,
            failures: 0,
            breaker: BreakerState::Closed,
            breaker_until: None,
            last_ok_at: None,
            last_error: None,
            samples: 0,
            timeout_streak: 0,
            probe_in_flight: false,
        }
    }
}

impl EngineHealth {
    fn from_row(row: &EngineHealthRow) -> Self {
        Self {
            ewma_ms: row.ewma_ms,
            failures: row.failures,
            breaker: row.breaker,
            breaker_until: row.breaker_until,
            last_ok_at: row.last_ok_at,
            last_error: row.last_error.clone(),
            // A persisted EWMA was already seeded; keep blending on top.
            samples: u64::from(row.ewma_ms > 0.0),
            // The schema stores only the generic `failures`; a restart
            // assumes no timeout streak rather than guessing one.
            timeout_streak: 0,
            probe_in_flight: false,
        }
    }

    fn to_row(&self, engine: &EngineId) -> EngineHealthRow {
        EngineHealthRow {
            engine: engine.clone(),
            ewma_ms: self.ewma_ms,
            failures: self.failures,
            breaker: self.breaker,
            breaker_until: self.breaker_until,
            last_ok_at: self.last_ok_at,
            last_error: self.last_error.clone(),
        }
    }

    fn observe(&mut self, latency: Duration) {
        let ms = latency.as_secs_f64() * 1_000.0;
        self.samples += 1;
        self.ewma_ms = if self.samples == 1 {
            ms
        } else {
            EWMA_ALPHA * ms + (1.0 - EWMA_ALPHA) * self.ewma_ms
        };
    }
}

/// The pipeline's per-engine gate decision (parent plan 4.4.6: skip `Open`,
/// probe `HalfOpen` with one call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Breaker closed: the engine fans out normally.
    Call,
    /// Breaker half-open: this is the single probe call.
    Probe,
    /// Breaker open (or a probe already in flight): do not call.
    Skip,
}

/// A breaker state change pending audit (`engine.breaker` rows, plan 6.1).
#[derive(Debug)]
struct Transition {
    engine: EngineId,
    from: BreakerState,
    to: BreakerState,
    until: Option<DateTime<Utc>>,
    error: Option<String>,
    request_id: Uuid,
}

#[derive(Default)]
struct Inner {
    map: HashMap<EngineId, EngineHealth>,
    /// Engines with unpersisted routine updates.
    dirty: HashSet<EngineId>,
    /// Breaker transitions awaiting audit + urgent persist.
    transitions: Vec<Transition>,
    last_flush: Option<Instant>,
}

/// Per-engine health state shared by the pipeline and the `/api/engines`
/// routes. Cheap to clone (all state behind one mutex).
pub struct HealthTracker {
    store: Arc<dyn Store>,
    policy: HealthPolicy,
    inner: Mutex<Inner>,
}

impl HealthTracker {
    /// Tracker with the settled policy (15 min abuse window, 3 timeouts,
    /// 5 min timeout window).
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_policy(store, HealthPolicy::default())
    }

    pub fn with_policy(store: Arc<dyn Store>, policy: HealthPolicy) -> Self {
        Self {
            store,
            policy,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Register a configured engine so it appears in [`snapshot`] (and
    /// `GET /api/engines`) before its first call.
    ///
    /// [`snapshot`]: Self::snapshot
    pub fn register(&self, id: &EngineId) {
        self.lock().map.entry(id.clone()).or_default();
    }

    /// Restore persisted `engine_health` rows at startup (parent plan 4.4.6:
    /// "a restart does not hammer a blocked engine"). Rows for engines no
    /// longer configured are loaded too — `/api/engines` reports them.
    /// Returns the number of rows applied.
    pub async fn load(&self) -> Result<usize, StoreError> {
        let rows = self.store.health().await?;
        let n = rows.len();
        let mut inner = self.lock();
        for row in &rows {
            *inner.map.entry(row.engine.clone()).or_default() = EngineHealth::from_row(row);
        }
        Ok(n)
    }

    /// Gate an engine call. May lazily transition `Open` -> `HalfOpen`
    /// once `breaker_until` passes (that transition is marked dirty and
    /// audited on the next flush). Unknown engines are treated as closed.
    pub fn admission(&self, id: &EngineId, request_id: Uuid) -> Admission {
        let mut inner = self.lock();
        let mut transition = None;
        let decision = {
            let health = inner.map.entry(id.clone()).or_default();
            match health.breaker {
                BreakerState::Closed => Admission::Call,
                BreakerState::Open => {
                    let elapsed = health
                        .breaker_until
                        .map(|until| Utc::now() >= until)
                        .unwrap_or(true);
                    if !elapsed {
                        debug!(engine = %id, "breaker open: skipping engine");
                        Admission::Skip
                    } else {
                        health.breaker = BreakerState::HalfOpen;
                        health.probe_in_flight = true;
                        transition = Some(Transition {
                            engine: id.clone(),
                            from: BreakerState::Open,
                            to: BreakerState::HalfOpen,
                            until: health.breaker_until,
                            error: health.last_error.clone(),
                            request_id,
                        });
                        info!(engine = %id, "breaker window elapsed: half-open probe");
                        Admission::Probe
                    }
                }
                BreakerState::HalfOpen => {
                    if health.probe_in_flight {
                        debug!(engine = %id, "half-open probe already in flight: skipping");
                        Admission::Skip
                    } else {
                        health.probe_in_flight = true;
                        Admission::Probe
                    }
                }
            }
        };
        if transition.is_some() {
            inner.dirty.insert(id.clone());
            inner.transitions.extend(transition);
        }
        decision
    }

    /// Record an answered call (`Ok` — including `NoResults`, which the
    /// engine answered healthily): EWMA update, failure counter reset, and
    /// a `HalfOpen` -> `Closed` transition when the call was a probe.
    pub fn record_ok(&self, id: &EngineId, latency: Duration, request_id: Uuid) {
        let mut inner = self.lock();
        let mut transition = None;
        {
            let health = inner.map.entry(id.clone()).or_default();
            health.observe(latency);
            health.failures = 0;
            health.timeout_streak = 0;
            health.last_ok_at = Some(Utc::now());
            health.probe_in_flight = false;
            if health.breaker != BreakerState::Closed {
                transition = Some((health.breaker, id.clone()));
                health.breaker = BreakerState::Closed;
                health.breaker_until = None;
            }
        }
        inner.dirty.insert(id.clone());
        if let Some((from, engine)) = transition {
            info!(engine = %engine, "breaker closed");
            inner.transitions.push(Transition {
                engine,
                from,
                to: BreakerState::Closed,
                until: None,
                error: None,
                request_id,
            });
        }
    }

    /// Record a failed call: EWMA update, failure counter increment,
    /// `last_error`, and the breaker rules — `RateLimited`/`Blocked` open
    /// for `abuse_window`, `timeout_threshold` consecutive `Timeout`s open
    /// for `timeout_window`, and a failed probe re-opens regardless of the
    /// error kind.
    pub fn record_err(
        &self,
        id: &EngineId,
        latency: Duration,
        err: &EngineError,
        request_id: Uuid,
    ) {
        let mut inner = self.lock();
        let mut transition = None;
        {
            let health = inner.map.entry(id.clone()).or_default();
            health.observe(latency);
            health.failures += 1;
            health.last_error = Some(err.to_string());
            // "3 consecutive timeouts" is a streak, not the generic
            // failure count: any other error kind (or an answer) resets it.
            health.timeout_streak = if matches!(err, EngineError::Timeout) {
                health.timeout_streak + 1
            } else {
                0
            };
            let probe = health.probe_in_flight;
            health.probe_in_flight = false;

            let open_for = if probe {
                // A failed probe re-opens immediately, whatever the kind.
                Some(self.open_window(err))
            } else {
                match err {
                    EngineError::RateLimited | EngineError::Blocked => {
                        Some(self.policy.abuse_window)
                    }
                    EngineError::Timeout
                        if health.timeout_streak >= self.policy.timeout_threshold =>
                    {
                        Some(self.policy.timeout_window)
                    }
                    _ => None,
                }
            };
            if let Some(window) = open_for {
                let until = Utc::now() + chrono::Duration::from_std(window).unwrap_or_default();
                if health.breaker != BreakerState::Open {
                    transition = Some(Transition {
                        engine: id.clone(),
                        from: health.breaker,
                        to: BreakerState::Open,
                        until: Some(until),
                        error: Some(err.to_string()),
                        request_id,
                    });
                }
                health.breaker = BreakerState::Open;
                health.breaker_until = Some(until);
            }
        }
        inner.dirty.insert(id.clone());
        if let Some(t) = transition {
            warn!(engine = %t.engine, error = %err, "breaker opened");
            inner.transitions.push(t);
        }
    }

    /// The probe call `admission` let through has finished (or was aborted
    /// when the request was cancelled): release the single-probe gate. A
    /// no-op for engines that were not probing.
    fn probe_finished(&self, id: &EngineId) {
        if let Some(health) = self.lock().map.get_mut(id) {
            health.probe_in_flight = false;
        }
    }

    /// A guard released when the engine call ends, however it ends —
    /// including request cancellation aborting the fan-out task, which
    /// would otherwise leave `probe_in_flight` stuck and the engine
    /// unprobeable until restart.
    pub fn probe_guard(self: &Arc<Self>, id: &EngineId) -> ProbeGuard {
        ProbeGuard {
            tracker: self.clone(),
            id: id.clone(),
        }
    }

    /// `POST /api/engines/{id}/reset`: restore a fresh `Closed` state and
    /// return `(previous breaker, fresh row)` for the audit details and the
    /// response body. `None` when the engine is unknown (not configured,
    /// no persisted row).
    pub fn reset(&self, id: &EngineId) -> Option<(BreakerState, EngineHealthRow)> {
        let mut inner = self.lock();
        let health = inner.map.get_mut(id)?;
        let previous = health.breaker;
        *health = EngineHealth::default();
        let row = health.to_row(id);
        inner.dirty.insert(id.clone());
        Some((previous, row))
    }

    /// Whether the engine is known (configured or persisted).
    pub fn contains(&self, id: &EngineId) -> bool {
        self.lock().map.contains_key(id)
    }

    /// The current row for one engine, if known.
    pub fn health_row(&self, id: &EngineId) -> Option<EngineHealthRow> {
        self.lock().map.get(id).map(|h| h.to_row(id))
    }

    /// All known engines as `engine_health` rows, sorted by engine id.
    pub fn snapshot(&self) -> Vec<EngineHealthRow> {
        let mut rows: Vec<_> = self.lock().map.iter().map(|(id, h)| h.to_row(id)).collect();
        rows.sort_by(|a, b| a.engine.cmp(&b.engine));
        rows
    }

    /// Write every dirty row through `Store::put_health` and append an
    /// `engine.breaker` audit row per pending transition. Rows whose write
    /// fails are re-marked dirty so the next flush retries them.
    pub async fn flush(&self) -> Result<(), StoreError> {
        let (rows, transitions) = {
            let mut inner = self.lock();
            inner.last_flush = Some(Instant::now());
            let dirty: Vec<EngineId> = inner.dirty.drain().collect();
            let rows: Vec<EngineHealthRow> = dirty
                .into_iter()
                .filter_map(|id| inner.map.get(&id).map(|h| h.to_row(&id)))
                .collect();
            (rows, std::mem::take(&mut inner.transitions))
        };

        let mut first_err = None;
        for row in rows {
            if let Err(e) = self.store.put_health(&row).await {
                warn!(engine = %row.engine, error = %e, "engine_health write failed");
                self.lock().dirty.insert(row.engine.clone());
                first_err.get_or_insert(e);
            }
        }
        for t in transitions {
            let row = AuditRow {
                id: None,
                ts: Utc::now(),
                actor: SYSTEM_ACTOR.to_string(),
                action: "engine.breaker".to_string(),
                target: t.engine.to_string(),
                details: serde_json::json!({
                    "from": t.from,
                    "to": t.to,
                    "until": t.until,
                    "error": t.error,
                }),
                request_id: Some(t.request_id),
            };
            // Same contract as oxe-server's audit helper: JSONL event
            // first, table write after; a failed insert is logged and
            // reported but does not fail the search.
            info!(
                target: "oxe.audit",
                audit = true,
                actor = %row.actor,
                action = %row.action,
                audit_target = %row.target,
                request_id = %t.request_id,
                "audit"
            );
            if let Err(e) = self.store.audit(row).await {
                warn!(error = %e, "engine.breaker audit write failed");
                first_err.get_or_insert(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Flush when a breaker transition is pending (urgent) or the last
    /// routine flush is at least `PERSIST_DEBOUNCE` old (never flushed
    /// counts as due). Called once per network search by the pipeline.
    pub async fn flush_due(&self) -> Result<(), StoreError> {
        let due = {
            let inner = self.lock();
            !inner.transitions.is_empty()
                || (!inner.dirty.is_empty()
                    && inner
                        .last_flush
                        .is_none_or(|t| t.elapsed() >= PERSIST_DEBOUNCE))
        };
        if due { self.flush().await } else { Ok(()) }
    }

    fn open_window(&self, err: &EngineError) -> Duration {
        match err {
            EngineError::RateLimited | EngineError::Blocked => self.policy.abuse_window,
            _ => self.policy.timeout_window,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned lock means a previous closure panicked; the health map
        // is still consistent enough to keep serving (fail open).
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Clears the `probe_in_flight` gate on drop — the normal path is the
/// engine task finishing, the abnormal one the request being cancelled and
/// the `JoinSet` aborting the task mid-await.
pub struct ProbeGuard {
    tracker: Arc<HealthTracker>,
    id: EngineId,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        self.tracker.probe_finished(&self.id);
    }
}
