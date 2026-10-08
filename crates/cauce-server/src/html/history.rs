//! `/history` feed page (W2-02): the `Accept: text/html` arm of the
//! shared history handler — the merged `search_log`/`click_log` feed
//! folded into display rows with day headers, cache state and nested
//! click lines.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::borrow::Cow;

use askama::Template;
use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::{CacheKey, ClickRow, HistoryItem, SearchOrigin};

use super::{STYLE_CSS, render_err};
use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::QueryParams;
use crate::middleware::RequestCtx;
use rust_i18n::t;

/// `Accept: text/html` (without an explicit JSON ask) wants the page — the
/// content-negotiation half of "the HTML page is the API handler".
pub(crate) fn prefers_html(headers: &HeaderMap) -> bool {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    accept.contains("text/html") && !accept.contains("application/json")
}

/// One clicked link nested under a search row (or the single line of a
/// `(click only)` row): domain, title link, result position.
#[derive(Debug)]
struct ClickLine {
    domain: String,
    url: String,
    title: String,
    /// 1-based `#n` position in the result list that was clicked.
    position: String,
}

/// One table row on the history page: a logged search, or a click whose
/// `query_hash` matches no shown search (rendered standalone so the feed
/// never hides recorded clicks).
#[derive(Debug)]
struct HistRow {
    /// A `search_log` row (carries clicks, rerun/json/payload actions).
    is_search: bool,
    /// An `answer_log` row (#254): query links to `/answer?q=`, the
    /// `engines` cell shows the model, `n` the source count.
    is_answer: bool,
    /// The delete endpoint — `/api/history/{id}` for searches,
    /// `/api/answer-log/{id}` for answers (#254).
    delete_url: String,
    /// `agent` chip next to the client label on search rows whose
    /// `origin` is `agent` (#254).
    origin_chip: bool,
    /// `ai` chip after the query on answer rows (#254).
    query_chip: String,
    /// The `c-query` cell link: `/search?q=` on searches,
    /// `/answer/{id}` on answers (#254 — durable past the `answers`
    /// TTL; the `re-run` action keeps the explicit `/answer?q=` re-ask).
    query_url: String,
    /// `YYYY-MM-DD` group header, set on the first row of each local day.
    day_header: Option<String>,
    /// `HH:MM` local time.
    when: String,
    /// The search query (click rows render `t!("history.click_only")` instead).
    query: String,
    /// `cached · <age>` | `cached · expired` | `network · t<N>`;
    /// `-` on click rows.
    source: String,
    /// `/cache?q=<query>#<key>` when the source is a live cache entry.
    source_url: String,
    /// Live cache entry → the `payload` action is shown.
    cached_live: bool,
    engines: String,
    result_count: String,
    latency: String,
    client: String,
    /// Nested click lines under this row.
    clicks: Vec<ClickLine>,
    /// `click` | `clicks` for the `<summary>` count (singular stays
    /// grammatical at 1).
    clicks_word: Cow<'static, str>,
    /// `hx-confirm` on the delete button; names the nested clicks so a
    /// destructive action never hides its blast radius.
    delete_confirm: String,
    rerun_url: String,
    json_url: String,
}

/// `/history` full page.
#[derive(Template)]
#[template(path = "history.html")]
struct History {
    /// The shared header's active nav item.
    nav_active: &'static str,
    /// W7-01: an answer loop exists — the header shows `/answer`.
    answer_available: bool,
    /// Selected `since` window (`24h` | `7d` | `30d` | `all`).
    since: String,
    /// Active `q` substring filter.
    q: String,
    /// `cached=1` filter.
    cached: bool,
    /// Any filter active → the `clear` link is shown.
    filters_active: bool,
    /// Selected `origin` filter as the select's value (`user` | `agent`
    /// | `all`) — #254; `user` is the page's implicit "mine" default.
    origin: String,
    /// `/history?origin=all` link in the empty state when the
    /// default-on `origin=user` filter hid every row (#254).
    origin_all_url: String,
    /// `N searches in last 24h · N total · N clicks today`.
    stats_line: String,
    /// Empty-state sentence (fresh store or filtered-empty wording).
    empty_message: String,
    rows: Vec<HistRow>,
    /// `showing 200 of N · use the filters to reach older searches`;
    /// empty unless the feed was truncated.
    capped_line: String,
    request_id: String,
    style_css: String,
}

/// `GET /history` (W2-02): identical to `GET /api/history` — the shared
/// handler negotiates on `Accept`.
pub async fn history(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    crate::handlers::history(State(state), Extension(ctx), uri, headers).await
}

/// The `Accept: text/html` arm of the shared history handler: same params,
/// same defaults, same row set as the JSON route, plus the render-time
/// reads the page needs (header stats, batched cache states).
pub(crate) async fn history_page(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    // #254: the page defaults `origin=user` ("mine") when the param is
    // absent; `/api/history` keeps the unfiltered JSON default.
    let (params, filter, items) = crate::handlers::history_inner(
        &state,
        &ctx,
        &uri,
        crate::handlers::HISTORY_LIMIT,
        Some(SearchOrigin::User),
    )
    .await?;
    let stats = state
        .store()
        .history_stats(&filter)
        .await
        .map_err(|e| ctx.store(&e))?;

    // One batched lookup for the page's distinct query_hashes — the
    // `source` column is computed at render time from `cache_entries`.
    let keys: Vec<CacheKey> = {
        let mut seen = std::collections::HashSet::new();
        items
            .iter()
            .filter_map(|i| match i {
                HistoryItem::Search(row) if seen.insert(row.query_hash.as_str()) => {
                    Some(row.query_hash.clone())
                }
                _ => None,
            })
            .collect()
    };
    let cache: std::collections::HashMap<String, cauce_core::CacheState> = state
        .store()
        .cache_states(&keys)
        .await
        .map_err(|e| ctx.store(&e))?
        .into_iter()
        .map(|st| (st.key.as_str().to_string(), st))
        .collect();

    // Clicks whose search row fell outside the rendered window (the feed
    // cap counts searches AND clicks) must not render as `(click only)` —
    // only a click with no `search_log` row at all is an orphan.
    let search_hashes_in_feed: std::collections::HashSet<String> = items
        .iter()
        .filter_map(|i| match i {
            HistoryItem::Search(row) => Some(row.query_hash.as_str().to_string()),
            _ => None,
        })
        .collect();
    let orphan_candidates: Vec<CacheKey> = {
        let mut seen = std::collections::HashSet::new();
        items
            .iter()
            .filter_map(|i| match i {
                HistoryItem::Click(c) => c.query_hash.as_ref().filter(|h| {
                    !search_hashes_in_feed.contains(h.as_str()) && seen.insert(h.as_str())
                }),
                _ => None,
            })
            .cloned()
            .collect()
    };
    let off_window: std::collections::HashSet<String> = state
        .store()
        .search_hashes(&orphan_candidates)
        .await
        .map_err(|e| ctx.store(&e))?
        .into_iter()
        .map(|k| k.as_str().to_string())
        .collect();

    let rows = history_rows(items, &cache, &off_window);
    // `origin != user` counts as a filter (#254): `origin=all` and
    // `origin=agent` both surface the clear link.
    let filters_active = filter.cached
        || filter.q.is_some()
        || filter.since.is_some()
        || filter.origin != Some(SearchOrigin::User);
    let (empty_message, origin_all_url) = if rows.is_empty() {
        // #254: `ef_origin` names the case where the default-on
        // `origin=user` filter emptied the feed — not a genuinely
        // empty store. Probe without the origin bound to tell them
        // apart (only pays for a second query on an empty page).
        if filter.origin == Some(SearchOrigin::User) {
            let probe = cauce_core::HistoryFilter {
                origin: None,
                ..filter.clone()
            };
            let unfiltered = state
                .store()
                .list_history(&probe)
                .await
                .map_err(|e| ctx.store(&e))?;
            if unfiltered.is_empty() {
                empty_message(&params, &probe)
            } else {
                (
                    t!("history.ef_origin").to_string(),
                    "/history?origin=all".to_string(),
                )
            }
        } else {
            empty_message(&params, &filter)
        }
    } else {
        (String::new(), String::new())
    };
    let capped_line = if stats.matching as usize > rows.len() && !rows.is_empty() {
        format!(
            "{} {} {} {} · {}",
            t!("history.capped_showing"),
            rows.len(),
            t!("history.capped_of"),
            stats.matching,
            t!("history.capped_hint")
        )
    } else {
        String::new()
    };
    let stats_line = format!(
        "{} {} · {} {} · {} {}",
        stats.searches_24h,
        t!("history.stat_searches_24h"),
        stats.searches_total,
        t!("history.stat_total"),
        stats.clicks_today,
        t!("history.stat_clicks_today"),
    );

    let page = History {
        nav_active: "history",
        answer_available: state.answer().is_some(),
        since: params.get("since").unwrap_or("all").to_string(),
        q: params.get("q").unwrap_or("").to_string(),
        cached: filter.cached,
        filters_active,
        origin: match filter.origin {
            Some(SearchOrigin::User) => "user",
            Some(SearchOrigin::Agent) => "agent",
            None => "all",
        }
        .to_string(),
        origin_all_url,
        stats_line,
        empty_message,
        rows,
        capped_line,
        request_id: ctx.request_id.as_uuid().to_string(),
        style_css: STYLE_CSS.clone(),
    };
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
}

/// The one-sentence empty state: plain `empty` when nothing is stored, or
/// the filtered-empty sentence naming the active filters. #254: the
/// `origin=user`-filtered-empty case is decided by the caller (it probes
/// the unfiltered store); this fn only sees the origin-free filter then.
fn empty_message(params: &QueryParams, filter: &cauce_core::HistoryFilter) -> (String, String) {
    if !filter.cached && filter.q.is_none() && filter.since.is_none() {
        return (t!("history.empty").to_string(), String::new());
    }
    let mut msg = match params.get("q").filter(|v| !v.trim().is_empty()) {
        Some(q) => format!("{} \"{q}\"", t!("history.ef_match")),
        None => t!("history.ef_none").to_string(),
    };
    match params.get("since") {
        Some("24h") => msg.push_str(&format!(" {}", t!("history.in_24h"))),
        Some("7d") => msg.push_str(&format!(" {}", t!("history.in_7d"))),
        Some("30d") => msg.push_str(&format!(" {}", t!("history.in_30d"))),
        // `all` adds no time clause; an absolute timestamp is spelled out.
        Some(v) if v != "all" => msg.push_str(&format!(" {} {v}", t!("history.in_since"))),
        _ => {}
    }
    if filter.cached {
        msg.push_str(&format!(" {}", t!("history.ef_cached")));
    }
    msg.push('.');
    (msg, String::new())
}

/// Compact entry age for `cached · <age>`: `42s`, `41m`, `3h`, `2d`.
fn cache_age(from: chrono::DateTime<chrono::Utc>, now: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (now - from).num_seconds().max(0) as u64;
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s => format!("{}d", s / 86_400),
    }
}

/// Fold the merged `list_history` feed into display rows: clicks join the
/// newest search row sharing their `query_hash`; clicks without one render
/// as standalone `(click only)` rows in feed position — except hashes in
/// `off_window`, which have a `search_log` row outside the rendered window
/// and are dropped rather than mislabeled. `cache` carries the render-time
/// `cache_entries` state for the page's distinct query_hashes.
fn history_rows(
    items: Vec<HistoryItem>,
    cache: &std::collections::HashMap<String, cauce_core::CacheState>,
    off_window: &std::collections::HashSet<String>,
) -> Vec<HistRow> {
    use std::collections::{HashMap, HashSet};

    let now = chrono::Utc::now();
    let search_hashes: HashSet<String> = items
        .iter()
        .filter_map(|i| match i {
            HistoryItem::Search(s) => Some(s.query_hash.as_str().to_string()),
            _ => None,
        })
        .collect();
    let mut clicks_by_hash: HashMap<String, Vec<ClickLine>> = HashMap::new();
    for item in &items {
        if let HistoryItem::Click(c) = item
            && let Some(hash) = &c.query_hash
            && search_hashes.contains(hash.as_str())
        {
            clicks_by_hash
                .entry(hash.as_str().to_string())
                .or_default()
                .push(click_line(c));
        }
    }

    let mut attached: HashSet<String> = HashSet::new();
    let mut last_day = String::new();
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        match item {
            HistoryItem::Search(s_row) => {
                let query = s_row
                    .query_raw
                    .clone()
                    .unwrap_or_else(|| s_row.query.clone());
                let encoded = urlencoding::encode(&query).into_owned();
                // First occurrence wins: items are newest-first, so clicks
                // attach to the newest search row with their query_hash.
                let clicks = if attached.insert(s_row.query_hash.as_str().to_string()) {
                    clicks_by_hash
                        .remove(s_row.query_hash.as_str())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (source, source_url, cached_live) = match cache.get(s_row.query_hash.as_str()) {
                    Some(st) if st.expires_at > now => {
                        let url = format!(
                            "/cache?q={}#{}",
                            urlencoding::encode(&st.query),
                            st.key.as_str()
                        );
                        (
                            format!(
                                "{} · {}",
                                t!("history.src_cached"),
                                cache_age(st.created_at, now)
                            ),
                            url,
                            true,
                        )
                    }
                    Some(_) => (
                        format!(
                            "{} · {}",
                            t!("history.src_cached"),
                            t!("history.src_expired")
                        ),
                        String::new(),
                        false,
                    ),
                    None => (
                        match s_row.tier {
                            Some(t) => {
                                format!(
                                    "{} · {}{}",
                                    t!("history.src_network"),
                                    t!("history.tier_prefix"),
                                    t.as_u8()
                                )
                            }
                            None => t!("history.src_network").to_string(),
                        },
                        String::new(),
                        false,
                    ),
                };
                let clicks_word = if clicks.len() == 1 {
                    t!("history.click_one")
                } else {
                    t!("history.clicks_word")
                };
                let delete_confirm = if clicks.is_empty() {
                    t!("history.delete_confirm").to_string()
                } else {
                    format!(
                        "{} {} {}?",
                        t!("history.delete_confirm_clicks_pre"),
                        clicks.len(),
                        clicks_word
                    )
                };
                let id = s_row.id.unwrap_or(0);
                rows.push(HistRow {
                    is_search: true,
                    is_answer: false,
                    delete_url: format!("/api/history/{id}"),
                    origin_chip: s_row.origin == SearchOrigin::Agent,
                    query_chip: String::new(),
                    day_header: day_header(s_row.ts, &mut last_day),
                    when: s_row
                        .ts
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string(),
                    query,
                    source,
                    source_url,
                    cached_live,
                    engines: s_row
                        .engines
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    result_count: s_row.result_count.to_string(),
                    latency: s_row.latency_ms.to_string(),
                    client: s_row.client.label(),
                    clicks,
                    clicks_word,
                    delete_confirm,
                    query_url: format!("/search?q={encoded}"),
                    rerun_url: format!("/search?q={encoded}"),
                    json_url: format!("/api/search?q={encoded}"),
                });
            }
            HistoryItem::Click(c) => {
                let absorbed = c.query_hash.as_ref().is_some_and(|h| {
                    search_hashes.contains(h.as_str()) || off_window.contains(h.as_str())
                });
                if absorbed {
                    continue;
                }
                rows.push(HistRow {
                    is_search: false,
                    is_answer: false,
                    delete_url: String::new(),
                    origin_chip: false,
                    query_chip: String::new(),
                    day_header: day_header(c.ts, &mut last_day),
                    when: c
                        .ts
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string(),
                    query: String::new(),
                    source: t!("common.dash").to_string(),
                    source_url: String::new(),
                    cached_live: false,
                    engines: t!("common.dash").to_string(),
                    result_count: t!("common.dash").to_string(),
                    latency: t!("common.dash").to_string(),
                    client: c.client.label(),
                    clicks: vec![click_line(&c)],
                    // Click-only rows render no `<summary>`; the word is
                    // unused there.
                    clicks_word: t!("history.clicks_word"),
                    delete_confirm: t!("history.delete_confirm").to_string(),
                    query_url: String::new(),
                    rerun_url: String::new(),
                    json_url: String::new(),
                });
            }
            HistoryItem::Answer(a) => {
                let query = a.query_raw.clone().unwrap_or_else(|| a.query.clone());
                let id = a.id.unwrap_or(0);
                let rerun = format!("/answer?q={}", urlencoding::encode(&query));
                // #254: the query cell links the durable `/answer/{id}`
                // render (survives the `answers` TTL); the `ask again`
                // action keeps `/answer?q=` as an explicit re-ask. The
                // `engines` cell shows the model, `n` the source count,
                // latency is a dash.
                rows.push(HistRow {
                    is_search: false,
                    is_answer: true,
                    delete_url: format!("/api/answer-log/{id}"),
                    origin_chip: false,
                    query_chip: t!("history.chip_ai").to_string(),
                    day_header: day_header(a.ts, &mut last_day),
                    when: a
                        .ts
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string(),
                    query,
                    source: a.status.label().to_string(),
                    source_url: String::new(),
                    cached_live: false,
                    engines: a.model.clone(),
                    result_count: a.sources.len().to_string(),
                    latency: t!("common.dash").to_string(),
                    client: a.client.label(),
                    clicks: Vec::new(),
                    clicks_word: t!("history.clicks_word"),
                    delete_confirm: t!("history.delete_confirm_answer").to_string(),
                    query_url: format!("/answer/{id}"),
                    rerun_url: rerun,
                    json_url: String::new(),
                });
            }
        }
    }
    rows
}

/// `YYYY-MM-DD` the first time each local day appears in the feed.
fn day_header(ts: chrono::DateTime<chrono::Utc>, last_day: &mut String) -> Option<String> {
    let day = ts
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    if day == *last_day {
        None
    } else {
        *last_day = day.clone();
        Some(day)
    }
}

/// One nested click line: domain, title (falling back to the URL), `#n`
/// (the beacon's 0-based position shown 1-based).
fn click_line(c: &ClickRow) -> ClickLine {
    ClickLine {
        domain: c.url.host_str().unwrap_or("").to_string(),
        url: c.url.as_str().to_string(),
        title: if c.title.is_empty() {
            c.url.as_str().to_string()
        } else {
            c.title.clone()
        },
        position: (c.position + 1).to_string(),
    }
}
