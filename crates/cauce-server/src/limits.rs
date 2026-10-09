//! PUB-01 admission guards for exposed instances: a per-client-IP token
//! bucket (`[rate_limit]`) and a global in-flight cap
//! (`server.max_inflight`).
//!
//! Both fail closed on the request, not the server: overflow gets the
//! `429` envelope + `Retry-After` the stale-serve overflow path already
//! uses elsewhere. Loopback peers and `Role::Admin` requests are exempt —
//! the operator's own tooling and a box-local health probe must never be
//! starved by internet traffic.
//!
//! The token bucket's `Quota` is frozen at router build (restart-required
//! keys); the on/off and proxy-trust switches are read from the live
//! config per request so `PUT /api/config` can toggle them.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Request, State};
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use cauce_core::config::{Config, RateLimitConfig};
use chrono::Datelike;
use governor::clock::{Clock, DefaultClock};
use governor::{DefaultKeyedRateLimiter, Quota};
use tokio::sync::Semaphore;

use crate::app::AppState;
use crate::capabilities::{Role, role_for};
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// The stateful half of the guards, built once on [`AppState`] from the
/// boot config. `ip` exists even when the limiter is off — `enabled` is
/// a live-read, so the bucket must be there when an operator flips it on.
#[derive(Clone)]
pub struct Limits {
    ip: Arc<DefaultKeyedRateLimiter<IpAddr>>,
    /// `server.max_inflight > 0`; `None` means unbounded (the default —
    /// local mode keeps its pre-PUB-01 behaviour bit-for-bit).
    inflight: Option<Arc<Semaphore>>,
    /// Rough rejected-request counts for observability (`/metrics`
    /// already exports counters; these land in logs via the span).
    rejected_rate: Arc<AtomicU64>,
    rejected_inflight: Arc<AtomicU64>,
    /// PUB-03: the per-(ip, UTC-day) counter behind
    /// `[ai].free_daily_answers` — in-memory like the bucket: a
    /// budget, not billing.
    free_answers: Arc<DailyCount>,
}

impl Limits {
    /// Build from the boot config. `requests_per_second = 0` clamps to 1
    /// (`Quota` is non-zero by construction; a 0 would be a config bug,
    /// not "off" — `enabled` is the off switch).
    pub fn from_config(cfg: &Config) -> Self {
        let quota = Quota::per_second(
            std::num::NonZeroU32::new(cfg.rate_limit.requests_per_second.max(1)).unwrap(),
        )
        .allow_burst(std::num::NonZeroU32::new(cfg.rate_limit.burst.max(1)).unwrap());
        Self {
            ip: Arc::new(DefaultKeyedRateLimiter::dashmap(quota)),
            inflight: (cfg.server.max_inflight > 0)
                .then(|| Arc::new(Semaphore::new(cfg.server.max_inflight as usize))),
            rejected_rate: Arc::new(AtomicU64::new(0)),
            rejected_inflight: Arc::new(AtomicU64::new(0)),
            free_answers: Arc::new(DailyCount::new()),
        }
    }

    /// PUB-03: spend one unit of `[ai].free_daily_answers` for this
    /// caller — `Ok` when the budget is unset (`0` = unlimited, the
    /// operator's explicit choice on a public instance), the caller is
    /// exempt (loopback peer, `Role::Admin` like the token bucket), or
    /// nothing attributes the request; `Err(Retry-After secs)` — the
    /// wait to UTC midnight — when today's budget is spent.
    pub fn check_free_answer(
        &self,
        cfg: &Config,
        headers: &HeaderMap,
        peer: Option<IpAddr>,
    ) -> Result<(), u64> {
        let limit = cfg.ai.free_daily_answers;
        if limit == 0
            || peer.is_some_and(|ip| ip.is_loopback())
            || matches!(role_for(cfg, headers), Role::Admin)
        {
            return Ok(());
        }
        let Some(ip) = client_key(headers, peer, &cfg.rate_limit) else {
            return Ok(());
        };
        self.free_answers.charge(ip, limit)
    }
}

/// The (ip, UTC-day) counts behind `check_free_answer`: the current
/// day and the per-IP tally in one lock so the midnight rollover is
/// atomic — a budget, not billing.
struct DailyCount {
    /// `num_days_from_ce` of the UTC day `counts` belongs to, and the
    /// day-scoped tallies.
    inner: Mutex<(i64, HashMap<IpAddr, u32>)>,
}

impl DailyCount {
    fn new() -> Self {
        Self {
            inner: Mutex::new((0, HashMap::new())),
        }
    }

    /// Spend one unit for `ip` today. `Err` carries the seconds to UTC
    /// midnight when the day's `limit` is already spent — a rejected
    /// request does not consume.
    fn charge(&self, ip: IpAddr, limit: u32) -> Result<(), u64> {
        let now = chrono::Utc::now();
        let today = i64::from(now.date_naive().num_days_from_ce());
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.0 != today {
            inner.0 = today;
            inner.1.clear();
        }
        let used = inner.1.entry(ip).or_insert(0);
        if *used >= limit {
            let tomorrow = (now.date_naive() + chrono::Days::new(1))
                .and_hms_opt(0, 0, 0)
                .expect("midnight exists")
                .and_utc();
            return Err((tomorrow - now).num_seconds().max(1) as u64);
        }
        *used += 1;
        Ok(())
    }
}

/// The client key for the bucket: `rate_limit.client_ip_header` when
/// configured, else `CF-Connecting-IP`, else the leftmost
/// `X-Forwarded-For` hop — but only when `rate_limit.trust_proxy_headers`
/// is set (headers are spoofable on a direct-exposed socket). Otherwise
/// the `ConnectInfo` peer. `None` when nothing attributes the request
/// (in-process `oneshot` calls); an unattributable request is exempt —
/// a public deployment always arrives over a socket.
fn client_key(
    headers: &HeaderMap,
    connect: Option<IpAddr>,
    cfg: &RateLimitConfig,
) -> Option<IpAddr> {
    let forwarded = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .and_then(|v| v.parse::<IpAddr>().ok())
    };
    if !cfg.trust_proxy_headers {
        return connect;
    }
    match cfg.client_ip_header.as_deref() {
        Some(name) => forwarded(name).or(connect),
        None => forwarded("cf-connecting-ip")
            .or_else(|| forwarded("x-forwarded-for"))
            .or(connect),
    }
}

/// Whether the request is exempt from both guards: `Role::Admin` (every
/// caller in local mode — the "local is unchanged" half of the
/// contract) or a loopback **peer** — the socket address, never a
/// forwarded header (a spoofed `X-Forwarded-For: 127.0.0.1` must not be
/// able to buy its way past the bucket).
fn exempt(state: &AppState, headers: &HeaderMap, peer: Option<IpAddr>) -> bool {
    if peer.is_some_and(|ip| ip.is_loopback()) {
        return true;
    }
    state.with_config(|cfg| matches!(role_for(cfg, headers), Role::Admin))
}

/// Whether the `[rate_limit]` bucket applies to this path: the API +
/// MCP surfaces only. Pages, redirects, `/health` and embedded assets
/// are cheap per hit and a browser's page load is ~10 requests — with
/// them counted, any sane burst setting would throttle real users.
fn api_scoped(path: &str) -> bool {
    path.starts_with("/api/") || path == "/mcp"
}

/// axum `from_fn_with_state` middleware: the per-IP token bucket.
/// Overflow answers `429 rate_limited` + `Retry-After` (the wait the
/// limiter itself reports, floor 1s).
pub async fn rate_limit_gate(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let headers = request.headers();
    let (enabled, cfg) = state.with_config(|cfg| {
        (
            cfg.rate_limit.enabled_for(cfg.server.public_instance),
            cfg.rate_limit.clone(),
        )
    });
    if !enabled || !api_scoped(request.uri().path()) {
        return next.run(request).await;
    }
    let connect = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    if exempt(&state, headers, connect) {
        return next.run(request).await;
    }
    let Some(ip) = client_key(headers, connect, &cfg) else {
        return next.run(request).await;
    };
    match state.limits().ip.check_key(&ip) {
        Ok(_) => next.run(request).await,
        Err(not_until) => {
            state.limits().rejected_rate.fetch_add(1, Ordering::Relaxed);
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            limited(
                &request,
                "rate_limited",
                "per-client rate limit exceeded",
                wait,
            )
        }
    }
}

/// axum `from_fn_with_state` middleware: the global in-flight cap.
/// `try_acquire` — saturation fails fast with `429 overloaded` +
/// `Retry-After: 1` rather than queueing unboundedly behind the
/// semaphore. `/health` stays exempt so a load balancer keeps seeing
/// the truth.
pub async fn inflight_gate(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(semaphore) = state.limits().inflight.clone() else {
        return next.run(request).await;
    };
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }
    let connect = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    if exempt(&state, request.headers(), connect) {
        return next.run(request).await;
    }
    let Ok(permit) = semaphore.try_acquire_owned() else {
        state
            .limits()
            .rejected_inflight
            .fetch_add(1, Ordering::Relaxed);
        return limited(
            &request,
            "overloaded",
            "server at its in-flight cap",
            std::time::Duration::from_secs(1),
        );
    };
    let response = next.run(request).await;
    drop(permit);
    response
}

/// The shared 429 shape: the standard `ApiError` envelope (request id
/// carried when `request_context` already ran) plus `Retry-After`.
fn limited(
    request: &Request<Body>,
    code: &'static str,
    message: &str,
    wait: std::time::Duration,
) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestCtx>()
        .map(|c| c.request_id.as_uuid());
    let mut response = ApiError::new(StatusCode::TOO_MANY_REQUESTS, code, message)
        .with_request_id(request_id)
        .into_response();
    response.headers_mut().insert(
        RETRY_AFTER,
        HeaderValue::from_str(&wait.as_secs().max(1).to_string())
            .expect("a small integer is valid header text"),
    );
    response
}
