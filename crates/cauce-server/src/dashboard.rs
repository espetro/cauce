//! `/dashboard` (W2-03): the observability page, rendered from the same
//! [`StatsSnapshot`] `/api/stats` serves — the handler delegates to
//! [`handlers::stats`] so the page never grows a second data path (settled
//! input). `Accept: application/json` proxies to the JSON handler outright.
//!
//! All numbers are formatted in Rust before they reach the template; the
//! askama context is plain display types only (percent strings, SVG rect
//! coordinates, breaker labels).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::Extension;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::{BreakerState, LatencyPercentiles, StatsSnapshot};

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers;
use crate::html::{STYLE_CSS, prefers_json, render_err};
use crate::middleware::RequestCtx;

/// SVG chart geometry (viewBox units; the bar chart is `0 0 W H`).
const CHART_W: f64 = 640.0;
const CHART_H: f64 = 100.0;
/// Top padding so a max-height bar never kisses the viewBox edge.
const CHART_PAD: f64 = 8.0;

/// One stacked bar of the searches-per-day chart: the `cache` segment sits
/// at the baseline, the `network` segment stacks on top. Coordinates are
/// preformatted strings so the template only interpolates.
#[derive(Debug)]
struct DayBar {
    x: String,
    w: String,
    cache_y: String,
    cache_h: String,
    net_y: String,
    net_h: String,
    /// `<title>` tooltip: "2026-09-23: 12 searches, 5 cached".
    title: String,
    /// `MM-DD` tick label under the bar.
    tick: String,
}

/// One `hits_by_tier` row: `tier 1 · 5 hits · 25% of searches`.
#[derive(Debug)]
struct TierRow {
    tier: u8,
    hits: u64,
    pct: String,
}

/// One `by_client` / `outcomes` row: `api · 12 · 75%`.
#[derive(Debug)]
struct SplitRow {
    name: String,
    count: u64,
    pct: String,
}

/// One `top_queries` row.
#[derive(Debug)]
struct QueryRow {
    query: String,
    searches: u64,
}

/// One `engines[]` table row; every cell preformatted.
#[derive(Debug)]
struct EngineRow {
    id: String,
    /// `/engines#<anchor>` link target: the card's encoded element id
    /// (`engine.<id>` through [`crate::settings::encode_id`]), so a
    /// dotted engine id still lands on its card.
    card_anchor: String,
    breaker: String,
    reliability: String,
    requests: u64,
    total: String,
    http: String,
    parse: String,
}

/// The `/dashboard` page context.
#[derive(Template)]
#[template(path = "dashboard.html")]
struct Dashboard {
    /// The shared header's active nav item.
    nav_active: &'static str,
    days: u32,
    has_data: bool,
    hit_rate_pct: String,
    total_hits: u64,
    total_searches: u64,
    bars: Vec<DayBar>,
    chart_w: String,
    chart_h: String,
    tier_rows: Vec<TierRow>,
    has_latency: bool,
    lat: LatencyPercentiles,
    has_ttfr: bool,
    ttfr: LatencyPercentiles,
    clients: Vec<SplitRow>,
    outcomes: Vec<SplitRow>,
    top_queries: Vec<QueryRow>,
    zero_queries: Vec<String>,
    deadline_hits: u64,
    deadline_rate: String,
    stale_served: u64,
    admission_rejected: u64,
    engines: Vec<EngineRow>,
    cache_rows: u64,
    cache_unexpired: u64,
    cache_expired: u64,
    cache_db: String,
    cache_newest: String,
    request_id: String,
    style_css: String,
}

/// `GET /dashboard?days=7|30` — stats page, or the `/api/stats` JSON when
/// the client asks for it (`prefers_json` content negotiation, same as
/// `/search`).
pub async fn dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if prefers_json(accept) {
        return handlers::stats(State(state), Extension(ctx), uri)
            .await
            .map(|j| j.into_response());
    }

    // The page reads the canonical stats handler, so the page and
    // `/api/stats` can never disagree (the v2 "two data planes" defect).
    let Json(snap) = handlers::stats(State(state), Extension(ctx.clone()), uri).await?;
    let rid = ctx.request_id.as_uuid().to_string();
    let page = Dashboard::from_snapshot(&snap, rid);
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
}

fn pct_str(part: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format!("{:.0}%", part as f64 / total as f64 * 100.0)
}

fn fmt_bytes(n: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if n >= GIB {
        format!("{:.1} GiB", n as f64 / GIB as f64)
    } else if n >= MIB {
        format!("{:.1} MiB", n as f64 / MIB as f64)
    } else if n >= KIB {
        format!("{:.1} KiB", n as f64 / KIB as f64)
    } else {
        format!("{n} B")
    }
}

fn breaker_label(state: BreakerState) -> &'static str {
    match state {
        BreakerState::Closed => "closed",
        BreakerState::HalfOpen => "half-open",
        BreakerState::Open => "open",
    }
}

/// `med/p80/p95` for one engine phase column ("-" when there are no samples
/// yet — the row still renders).
fn phase_cell(p: cauce_core::PhaseStats, has_samples: bool) -> String {
    if !has_samples {
        return "-".to_string();
    }
    format!("{}/{}/{}", p.median_ms, p.p80_ms, p.p95_ms)
}

fn f1(v: f64) -> String {
    format!("{v:.1}")
}

impl Dashboard {
    fn from_snapshot(snap: &StatsSnapshot, request_id: String) -> Self {
        let searches = snap.searches;

        let bars = day_bars(&snap.per_day);
        let tier_rows = snap
            .hits_by_tier
            .iter()
            .map(|t| TierRow {
                tier: t.tier,
                hits: t.hits,
                pct: pct_str(t.hits, searches),
            })
            .collect();

        let client_total: u64 = snap.by_client.iter().map(|c| c.searches).sum();
        let clients = snap
            .by_client
            .iter()
            .map(|c| SplitRow {
                name: c.client.clone(),
                count: c.searches,
                pct: pct_str(c.searches, client_total),
            })
            .collect();

        // `ok` first, then `error`/`rejected`, then any future label —
        // the split the amendment wants visible.
        let outcome_total: u64 = snap.outcomes.values().sum();
        let mut outcomes: Vec<SplitRow> = Vec::new();
        for name in ["ok", "error", "rejected"] {
            if let Some(n) = snap.outcomes.get(name) {
                outcomes.push(SplitRow {
                    name: name.to_string(),
                    count: *n,
                    pct: pct_str(*n, outcome_total),
                });
            }
        }
        for (name, n) in &snap.outcomes {
            if !matches!(name.as_str(), "ok" | "error" | "rejected") {
                outcomes.push(SplitRow {
                    name: name.clone(),
                    count: *n,
                    pct: pct_str(*n, outcome_total),
                });
            }
        }

        let top_queries = snap
            .top_queries
            .iter()
            .map(|q| QueryRow {
                query: q.query.clone(),
                searches: q.searches,
            })
            .collect();

        let engines = snap
            .engines
            .iter()
            .map(|e| EngineRow {
                id: e.engine.to_string(),
                card_anchor: crate::settings::encode_id(&format!("engine.{}", e.engine)),
                breaker: breaker_label(e.breaker).to_string(),
                reliability: format!("{:.0}%", e.reliability_pct),
                requests: e.requests,
                total: phase_cell(
                    cauce_core::PhaseStats {
                        median_ms: e.median_ms,
                        p80_ms: e.p80_ms,
                        p95_ms: e.p95_ms,
                    },
                    e.requests > 0,
                ),
                http: phase_cell(e.http, e.requests > 0),
                parse: phase_cell(e.parse, e.requests > 0),
            })
            .collect();

        Self {
            nav_active: "dashboard",
            days: snap.window_days,
            has_data: searches > 0,
            hit_rate_pct: format!("{:.0}%", snap.hit_rate * 100.0),
            total_hits: snap.cache_hits,
            total_searches: searches,
            bars,
            chart_w: f1(CHART_W),
            chart_h: f1(CHART_H + 14.0),
            tier_rows,
            has_latency: snap.latency.is_some(),
            lat: snap.latency.unwrap_or_default(),
            has_ttfr: snap.ttfr.is_some(),
            ttfr: snap.ttfr.unwrap_or_default(),
            clients,
            outcomes,
            top_queries,
            zero_queries: snap.zero_result_queries.clone(),
            // Windowed numerator over the windowed `searches` denominator
            // (`search_log.deadline_hit`), never the lifetime metrics
            // counter — the two disagree whenever the process outlives the
            // window.
            deadline_hits: snap.deadline_hits,
            deadline_rate: pct_str(snap.deadline_hits, searches),
            // `stale_served` exists only as a lifetime counter (no
            // `search_log` column), so it renders as a bare count like
            // `admission_rejected`, not a windowed rate.
            stale_served: snap.admission.stale_served,
            admission_rejected: snap.admission.rejected,
            engines,
            cache_rows: snap.cache_entries + snap.cache_entries_expired,
            cache_unexpired: snap.cache_entries,
            cache_expired: snap.cache_entries_expired,
            cache_db: fmt_bytes(snap.cache_db_bytes),
            cache_newest: snap
                .cache_newest_at
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "-".to_string()),
            request_id,
            style_css: STYLE_CSS.clone(),
        }
    }
}

/// Stacked bars for the searches-per-day panel: cache hits at the baseline
/// (`accent`), network on top (`muted`). Empty when the window has no rows.
fn day_bars(per_day: &[cauce_core::DayCount]) -> Vec<DayBar> {
    let n = per_day.len();
    let max = per_day.iter().map(|d| d.searches).max().unwrap_or(0);
    if n == 0 || max == 0 {
        return Vec::new();
    }
    let usable = CHART_H - CHART_PAD;
    let slot = CHART_W / n as f64;
    let bw = (slot * 0.6).min(28.0);
    per_day
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let x = i as f64 * slot + (slot - bw) / 2.0;
            let cache_h = d.cache_hits as f64 / max as f64 * usable;
            let net_h = (d.searches - d.cache_hits) as f64 / max as f64 * usable;
            DayBar {
                x: f1(x),
                w: f1(bw),
                cache_y: f1(CHART_H - cache_h),
                cache_h: f1(cache_h),
                net_y: f1(CHART_H - cache_h - net_h),
                net_h: f1(net_h),
                title: format!(
                    "{}: {} searches, {} cached",
                    d.day, d.searches, d.cache_hits
                ),
                tick: d.day.format("%m-%d").to_string(),
            }
        })
        .collect()
}
