//! `/engines` page (W2-05): one card per configured engine — kind, tier,
//! enabled, breaker state with remaining time, EWMA, last ok, last error,
//! p95, reliability, requests today — plus the three operator actions the
//! plan allows: reset the breaker, run an inline test query, enable or
//! disable the engine.
//!
//! The page is the `Accept: text/html` arm of `handlers::engines_list`,
//! so `/engines` and `GET /api/engines` share one handler and one data
//! plane ([`engine_views`]); the reset/enable/disable `POST`s answer the
//! re-rendered card partial for `hx-swap="outerHTML"`, and the test form
//! hits `GET /api/search` under `Accept: text/html` (the shared data
//! path, not a bespoke endpoint). Copy lives in `strings::engines`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::response::{Html, IntoResponse, Response};
use cauce_core::{BreakerState, EngineHealthRow, EngineId};
use chrono::Utc;

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::{EngineView, engine_views};
use crate::html::{HTMX_JS, JSON_ENC_JS, STYLE_CSS, render_err};
use crate::middleware::RequestCtx;
use crate::strings::{common, engines as copy};

/// One `/engines` card. Plain strings so Askama only needs `Display`; the
/// card doubles as the `outerHTML` swap target for the card's actions.
#[derive(Debug)]
pub(crate) struct EngineCard {
    id: String,
    /// Header meta in the spec's short form (`exec · t2`); a missing half
    /// renders just the known piece, both missing render `-`.
    kind_tier: String,
    /// `yes` | `no` for the stats list.
    enabled_label: &'static str,
    /// The resolved config names this engine (file entry or built-in).
    configured: bool,
    /// Live in the running pipeline's fan-out set.
    live: bool,
    /// `Closed` | `Open` | `HalfOpen` (the exact chip words the
    /// checkpoints assert on).
    breaker: &'static str,
    /// CSS class fragment of `breaker` (`closed`/`open`/`half-open`).
    breaker_class: &'static str,
    /// `retries in 42s` next to an open chip, `probing` next to a
    /// half-open one, empty otherwise.
    breaker_note: String,
    ewma: String,
    /// `HH:MM (rel)` local time, or `-` when unseen.
    last_ok: String,
    last_error: String,
    p95: String,
    /// `0.62`-style ratio (`reliability_pct / 100`), or `-`.
    reliability: String,
    /// Searches served by this engine since UTC midnight (`search_log`).
    requests_today: u64,
    /// `POST` target for the breaker reset; rendered only when
    /// [`Self::can_reset`] is true (the tracker 404s unknown engines).
    reset_url: String,
    /// The health tracker knows this engine (registered or a persisted
    /// row): `POST /api/engines/{id}/reset` answers 200. Untracked cards
    /// hide the button instead of offering a call that 404s.
    can_reset: bool,
    /// `POST` target flipping `enabled`; label is the action that happens.
    toggle_url: String,
    toggle_label: &'static str,
    /// `CAUCE_ENGINES` pins the set: the toggle renders disabled with the
    /// `pinned by CAUCE_ENGINES` hint.
    pinned: bool,
    /// `data-request-id` on the card so a swapped-in fragment traces back
    /// to the action that rendered it.
    request_id: String,
    /// Post-toggle hint (`saved; applies after restart`); empty on a
    /// plain page render.
    notice: String,
}

/// `engines.html` — the full page shell.
#[derive(Template)]
#[template(path = "engines.html")]
struct EnginesPage {
    /// The shared header's active nav item.
    nav_active: &'static str,
    cards: Vec<EngineCard>,
    /// `N configured · N enabled[ · N breaker open]` under the heading.
    summary: String,
    /// `CAUCE_ENGINES` pins the enabled set; toggles still write the file
    /// but the page explains why resolved `enabled` does not move.
    engines_pinned: bool,
    /// Full page-render request id (footer, copyable).
    request_id: String,
    htmx_js: String,
    json_enc_js: String,
    style_css: String,
}

/// `engine_card.html` — one card, also the HX action response.
#[derive(Template)]
#[template(path = "engine_card.html")]
struct EngineCardPartial {
    card: EngineCard,
}

/// The `Accept: text/html` arm of `handlers::engines_list` (`GET
/// /engines` and `GET /api/engines` share that handler): the card grid
/// page reading the shared [`engine_views`] plane.
pub(crate) async fn page(state: &AppState, ctx: &RequestCtx) -> Result<Html<String>, ApiError> {
    let views = engine_views(state).await?;
    let pinned = engines_pinned();
    let rid = ctx.request_id.as_uuid().to_string();
    let cards = views
        .iter()
        .map(|v| card_view(v, pinned, &rid, None))
        .collect();

    let configured = views.iter().filter(|v| v.configured).count();
    let enabled = views.iter().filter(|v| v.enabled).count();
    let open = views
        .iter()
        .filter(|v| v.health.breaker == BreakerState::Open)
        .count();
    let mut summary = copy::SUMMARY
        .replace("{configured}", &configured.to_string())
        .replace("{enabled}", &enabled.to_string());
    if open > 0 {
        summary.push_str(&copy::SUMMARY_OPEN.replace("{open}", &open.to_string()));
    }

    EnginesPage {
        nav_active: "engines",
        cards,
        summary,
        engines_pinned: pinned,
        request_id: rid,
        htmx_js: HTMX_JS.clone(),
        json_enc_js: JSON_ENC_JS.clone(),
        style_css: STYLE_CSS.clone(),
    }
    .render()
    .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
    .map(Html)
}

/// The re-rendered card partial an HX action response swaps in
/// (`hx-swap="outerHTML"` on `.engine-card`). Called by the
/// `POST /api/engines/{id}/reset` and `.../{enable,disable}` handlers;
/// `notice` is the post-toggle hint (`saved; applies after restart`).
pub(crate) async fn card(
    state: &AppState,
    id: &EngineId,
    request_id: uuid::Uuid,
    notice: Option<&'static str>,
) -> Result<Response, ApiError> {
    let views = engine_views(state).await?;
    let Some(view) = views.iter().find(|v| v.health.engine == *id) else {
        return Err(
            ApiError::not_found(format!("no such engine {id}")).with_request_id(Some(request_id))
        );
    };
    let card = card_view(view, engines_pinned(), &request_id.to_string(), notice);
    EngineCardPartial { card }
        .render()
        .map(|h| Html(h).into_response())
        .map_err(|e| render_err(e, request_id))
}

/// `CAUCE_ENGINES` pins the enabled set (settled input: it beats the
/// config file), so the toggle buttons are disabled while it is set.
fn engines_pinned() -> bool {
    std::env::var("CAUCE_ENGINES").is_ok_and(|v| !v.trim().is_empty())
}

/// One [`EngineView`] row rendered to template strings.
fn card_view(
    v: &EngineView,
    pinned: bool,
    request_id: &str,
    notice: Option<&'static str>,
) -> EngineCard {
    let id = v.health.engine.to_string();
    let (breaker, breaker_class, mut breaker_note) = breaker_fields(&v.health);
    if !v.enabled && breaker_note.is_empty() {
        // The mockup's `[ Closed ] disabled`: a closed breaker on a
        // disabled engine still flags the off state at a glance.
        breaker_note = copy::DISABLED.to_string();
    }
    // `exec · t2` (the mockup's short form); a missing half renders just
    // the known piece, both missing render `-`.
    let kind_tier = match (v.kind.as_str(), v.tier) {
        ("-", None) => common::DASH.to_string(),
        (kind, Some(t)) => format!("{kind} · t{t}"),
        (kind, None) => kind.to_string(),
    };
    EngineCard {
        reset_url: format!("/api/engines/{}/reset", urlencoding::encode(&id)),
        can_reset: v.tracked,
        toggle_url: format!(
            "/api/engines/{}/{}",
            urlencoding::encode(&id),
            if v.enabled { "disable" } else { "enable" }
        ),
        toggle_label: if v.enabled {
            copy::ACTION_DISABLE
        } else {
            copy::ACTION_ENABLE
        },
        enabled_label: if v.enabled {
            copy::ENABLED_YES
        } else {
            copy::ENABLED_NO
        },
        id,
        kind_tier,
        configured: v.configured,
        live: v.live,
        breaker,
        breaker_class,
        breaker_note,
        ewma: stat_ms(v.health.ewma_ms),
        last_ok: last_ok(&v.health),
        last_error: v
            .health
            .last_error
            .as_deref()
            .map(|e| e.chars().take(160).collect())
            .unwrap_or_else(|| common::DASH.to_string()),
        // `p95_ms` is `Some` only once the engine has served a request,
        // so `Some(0)` is a real sub-millisecond sample (u32 truncation),
        // not "unseen" — render `<1 ms`, never `0 ms`.
        p95: v
            .p95_ms
            .map(|ms| {
                if ms == 0 {
                    copy::SUB_MS.to_string()
                } else {
                    format!("{} ms", thousands(ms as u64))
                }
            })
            .unwrap_or_else(|| common::DASH.to_string()),
        reliability: v
            .reliability_pct
            .map(|p| format!("{:.2}", p / 100.0))
            .unwrap_or_else(|| common::DASH.to_string()),
        requests_today: v.requests_today,
        pinned,
        request_id: request_id.to_string(),
        notice: notice.unwrap_or_default().to_string(),
    }
}

/// Chip label, CSS class and side note derived from a health row.
fn breaker_fields(row: &EngineHealthRow) -> (&'static str, &'static str, String) {
    match row.breaker {
        BreakerState::Closed => (copy::BREAKER_CLOSED, "closed", String::new()),
        BreakerState::HalfOpen => (
            copy::BREAKER_HALF_OPEN,
            "half-open",
            copy::BREAKER_PROBING.to_string(),
        ),
        BreakerState::Open => {
            let note = row
                .breaker_until
                .map(|until| {
                    let left = until - Utc::now();
                    if left.num_seconds() <= 0 {
                        // The lazy `Open -> HalfOpen` truth: an elapsed
                        // window admits the next call as the probe.
                        copy::BREAKER_ELAPSED.to_string()
                    } else {
                        copy::BREAKER_RETRIES
                            .replace("{rel}", &human_seconds(left.num_seconds() as u64))
                    }
                })
                .unwrap_or_default();
            (copy::BREAKER_OPEN, "open", note)
        }
    }
}

/// `last ok` cell: local `HH:MM` plus the relative age (`01:02 (9m)`),
/// `-` when the scheduler has not seen a success.
fn last_ok(row: &EngineHealthRow) -> String {
    row.last_ok_at
        .map(|t| {
            let age = (Utc::now() - t).num_seconds().max(0) as u64;
            copy::LAST_OK_FMT
                .replace(
                    "{hhmm}",
                    &t.with_timezone(&chrono::Local).format("%H:%M").to_string(),
                )
                .replace("{rel}", &human_seconds(age))
        })
        .unwrap_or_else(|| common::DASH.to_string())
}

/// EWMA cell: `1 840 ms`, or `-` while the tracker is at zero.
fn stat_ms(ms: f64) -> String {
    if ms > 0.0 {
        format!("{} ms", thousands(ms.round() as u64))
    } else {
        common::DASH.to_string()
    }
}

/// `1840` -> `1 840` (the spec's grouped-thousands ms style).
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Seconds -> `45s` / `12m` / `3h` / `2d`.
fn human_seconds(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}
