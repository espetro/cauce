//! Admission control (W1-07, parent plan section 6.2).
//!
//! Two mechanisms guard the upstream fan-out:
//!
//! 1. **Singleflight on `CacheKey`.** The first request to miss the tier-1
//!    cache for a key is elected *leader*: it registers a `watch` channel in
//!    the in-flight map and spawns a detached flight task that does the real
//!    work (permit wait, fan-out, persist, stale fallback). Every later
//!    request with the same key is a *follower* and simply awaits the
//!    published [`FlightResult`]. The detached task means a cancelled leader
//!    request does not strand its followers — the fetch still completes,
//!    publishes and persists.
//! 2. **Bounded per-engine queue.** Each engine id owns a `Semaphore` with
//!    `max_concurrent_per_engine` permits; a flight must hold one permit per
//!    engine call it spawns before calling upstream — the t=0 wave
//!    upfront, a deferred (tier-2) engine inside the fetch when its
//!    hedge triggers. Waits are FIFO (tokio semaphore fairness) and
//!    bounded by `max_wait` in total. A flight that cannot acquire inside
//!    the budget *overflows*: the pipeline then serves
//!    the stored row for the key if one exists (`Source::Cache{stale:true}`
//!    when expired, plus a background refresh) else
//!    [`PipelineError::RateLimited`].
//!
//! Metric hooks (W1-09) are plain `tracing` events for now:
//! `admission.wait_ms` on every acquire, `admission_rejected{reason}` on
//! overflow, `stale_served` when an expired row goes out,
//! `singleflight_leader`/`singleflight_join` per election.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tracing::{debug, info};

use crate::cache::CacheKey;
use crate::engine::EngineId;
use crate::pipeline::PipelineError;
use crate::response::SearchResponse;

/// What a flight publishes once: every waiter receives the same outcome.
/// The `Arc` keeps 20 waiters from deep-copying a page of results; each
/// still rewrites `meta.request_id`/`elapsed_ms` on its own copy.
pub type FlightResult = Result<Arc<SearchResponse>, PipelineError>;

/// `admission.*` limits (parent plan 6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionLimits {
    /// Total time a flight may spend waiting for per-engine permits before
    /// it overflows (`admission.max_wait_ms`, default 1500 ms).
    pub max_wait: Duration,
    /// Concurrent upstream calls allowed per engine id. Default 3 matches
    /// the per-engine politeness burst (settled inputs: 1 req/s burst 3).
    pub max_concurrent_per_engine: usize,
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            max_wait: Duration::from_millis(1_500),
            max_concurrent_per_engine: 3,
        }
    }
}

/// Permit-acquisition budget for a background refresh (stale-serve path).
/// Unlike request flights — whose wait is `max_wait` because a user is
/// blocked on the outcome — a refresh has nobody waiting on it, so it may
/// sit in the FIFO much longer before giving up.
pub(crate) const REFRESH_MAX_WAIT: Duration = Duration::from_secs(30);

/// The shared admission state: in-flight map + per-engine semaphores.
/// Cheap to clone (all state lives behind one `Arc`); the pipeline keeps
/// one and clones it into spawned flight/refresh tasks.
#[derive(Clone)]
pub struct Admission {
    inner: Arc<Inner>,
}

struct Inner {
    limits: AdmissionLimits,
    /// `CacheKey` -> the channel the leader's flight task will publish to.
    flights: Mutex<HashMap<CacheKey, FlightSlot>>,
    /// Lazily created per-engine permit pools.
    semaphores: Mutex<HashMap<EngineId, Arc<Semaphore>>>,
    /// Monotonic flight id: lets a `Lead` remove *its own* map entry and
    /// never a later flight that re-registered the same key.
    next_flight: AtomicU64,
}

struct FlightSlot {
    id: u64,
    tx: watch::Sender<Option<FlightResult>>,
}

impl Default for Admission {
    fn default() -> Self {
        Self::new(AdmissionLimits::default())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned lock only means a previous holder panicked mid-mutation;
    // the maps stay structurally valid (entries are inserted whole), so
    // recovering is safe.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Admission {
    pub fn new(limits: AdmissionLimits) -> Self {
        Self {
            inner: Arc::new(Inner {
                limits,
                flights: Mutex::new(HashMap::new()),
                semaphores: Mutex::new(HashMap::new()),
                next_flight: AtomicU64::new(0),
            }),
        }
    }

    /// The configured limits.
    pub fn limits(&self) -> AdmissionLimits {
        self.inner.limits
    }

    /// `retry_after_s` for `PipelineError::RateLimited`: the wait budget in
    /// seconds (floor 1) — by then at least one flight should have left the
    /// queue. W1-06 engine EWMA can refine this later.
    pub fn retry_after_s(&self) -> u64 {
        self.inner.limits.max_wait.as_secs().max(1)
    }

    /// Enter the flight for `key`: the caller is either the elected
    /// *leader* (`Some(Lead)` — it must spawn the fetch and publish via
    /// [`Lead::complete`]) or a *follower* holding a receiver that resolves
    /// once the leader publishes.
    ///
    /// Election is atomic under the map lock: two concurrent misses for the
    /// same key can never both lead.
    pub(crate) fn enter(
        &self,
        key: &CacheKey,
    ) -> (Option<Lead>, watch::Receiver<Option<FlightResult>>) {
        let mut flights = lock(&self.inner.flights);
        match flights.entry(key.clone()) {
            Entry::Occupied(slot) => {
                debug!(key = %key, singleflight_join = true, "admission: joining in-flight search");
                (None, slot.get().tx.subscribe())
            }
            Entry::Vacant(slot) => {
                let id = self.inner.next_flight.fetch_add(1, Ordering::Relaxed);
                let (tx, rx) = watch::channel(None);
                slot.insert(FlightSlot { id, tx: tx.clone() });
                debug!(key = %key, singleflight_leader = true, "admission: leading new flight");
                (
                    Some(Lead {
                        key: key.clone(),
                        id,
                        tx,
                        admission: self.clone(),
                        completed: false,
                    }),
                    rx,
                )
            }
        }
    }

    /// Acquire one permit per engine in `engines`, in list order (callers
    /// pass the pipeline's configured order, so all flights acquire in the
    /// same order and cannot deadlock), waiting at most
    /// `limits.max_wait` *in total*.
    pub(crate) async fn acquire(&self, engines: &[EngineId]) -> Result<EnginePermits, WaitTimeout> {
        self.acquire_within(engines, self.inner.limits.max_wait)
            .await
    }

    /// [`acquire`] with an explicit budget (the background refresh uses
    /// [`REFRESH_MAX_WAIT`]).
    pub(crate) async fn acquire_within(
        &self,
        engines: &[EngineId],
        wait: Duration,
    ) -> Result<EnginePermits, WaitTimeout> {
        let semaphores = self.semaphores(engines);
        let started = Instant::now();
        match tokio::time::timeout(wait, acquire_all(&semaphores)).await {
            Ok(permits) => {
                debug!(
                    admission_wait_ms = started.elapsed().as_millis() as u64,
                    engines = engines.len(),
                    "admission: engine permits acquired"
                );
                Ok(EnginePermits { _held: permits })
            }
            Err(_) => {
                info!(
                    admission_rejected = true,
                    reason = "wait_timeout",
                    wait_ms = wait.as_millis() as u64,
                    engines = engines.len(),
                    "admission: queue wait exceeded"
                );
                Err(WaitTimeout)
            }
        }
    }

    /// The `Arc<Semaphore>` for each id, creating pools lazily. Duplicate
    /// ids (two configured engines sharing one id) yield the same semaphore
    /// twice — the caller then holds two permits, which is the correct
    /// concurrency accounting.
    fn semaphores(&self, engines: &[EngineId]) -> Vec<Arc<Semaphore>> {
        let mut map = lock(&self.inner.semaphores);
        engines
            .iter()
            .map(|id| {
                map.entry(id.clone())
                    .or_insert_with(|| {
                        Arc::new(Semaphore::new(self.inner.limits.max_concurrent_per_engine))
                    })
                    .clone()
            })
            .collect()
    }
}

/// The wait budget elapsed before all engine permits were held.
#[derive(Debug)]
pub(crate) struct WaitTimeout;

/// Held permits; never read, they exist to be dropped when the flight
/// finishes.
pub(crate) struct EnginePermits {
    _held: Vec<OwnedSemaphorePermit>,
}

async fn acquire_all(semaphores: &[Arc<Semaphore>]) -> Vec<OwnedSemaphorePermit> {
    let mut held = Vec::with_capacity(semaphores.len());
    for sem in semaphores {
        // `acquire_owned` only fails on a closed semaphore; nothing in this
        // codebase ever closes one.
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .expect("admission semaphores are never closed");
        held.push(permit);
    }
    held
}

/// Leader token for one flight. Dropping it without [`Lead::complete`]
/// (task panic/abort) frees the map slot and closes the channel so waiters
/// wake up and re-elect instead of hanging forever.
pub(crate) struct Lead {
    key: CacheKey,
    id: u64,
    tx: watch::Sender<Option<FlightResult>>,
    admission: Admission,
    completed: bool,
}

impl Lead {
    /// Publish `result` to every waiter and free the flight slot. Send
    /// first, remove second: subscribers must observe the value before the
    /// slot disappears.
    pub(crate) fn complete(mut self, result: FlightResult) {
        // Err only when there are no receivers; the outcome is moot then.
        let _ = self.tx.send(Some(result));
        self.remove_entry();
        self.completed = true;
    }

    /// Remove this flight's map entry — but only if it is still *this*
    /// flight (a re-elected leader for the same key must survive).
    fn remove_entry(&self) {
        let mut flights = lock(&self.admission.inner.flights);
        if flights
            .get(&self.key)
            .is_some_and(|slot| slot.id == self.id)
        {
            flights.remove(&self.key);
        }
    }
}

impl Drop for Lead {
    fn drop(&mut self) {
        if !self.completed {
            self.remove_entry();
        }
    }
}
