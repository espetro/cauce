//! The `/` + `/search` page domain: the search page shell (`index`,
//! `search`), its result rows, badge and engine-status line, the
//! streaming shell's URL and string bundle, and the `Accept: text/html`
//! arm of `GET /api/search` (the engines page's inline test fragment).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::{CacheKey, EngineError, EngineStatus, SearchRequest, SearchResponse, Source};
use serde_json::json;

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::{QueryParams, search_inner};
use crate::middleware::RequestCtx;

use super::assets::STYLE_CSS;
use super::{Page, Row, is_htmx, prefers_json, render_err, render_html, short_id};

/// Results partial swapped in by HTMX `hx-get` on the more button.
#[derive(Template)]
#[template(path = "results.html")]
struct Results {
    results: Vec<Row>,
    more_url: String,
    show_empty: bool,
    empty_status: String,
}

/// `GET /` landing page with the search form.
pub async fn index(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> Result<Html<String>, ApiError> {
    let rid = ctx.request_id.as_uuid().to_string();
    let page = Page {
        nav_active: "search",
        answer_available: state.answer().is_some(),
        q: String::new(),
        has_results: false,
        show_empty: false,
        empty_status: String::new(),
        result_count: 0,
        badge: String::new(),
        request_id: rid.clone(),
        short_request_id: short_id(&rid),
        results: Vec::new(),
        more_url: String::new(),
        style_css: STYLE_CSS.clone(),
        is_streaming: false,
        stream_url: String::new(),
        query_hash: String::new(),
        stream_strings: stream_strings(),
        ask_url: String::new(),
        index_on_click: state.archive_index_on_click(),
        assist: false,
        assist_context: "[]".to_string(),
        assist_strings: String::new(),
    };
    render_html(page, ctx.request_id.as_uuid())
}

/// `GET /search?q=...` with content negotiation and HTMX partial support.
pub async fn search(
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
        return crate::handlers::search(State(state), Extension(ctx), uri, headers).await;
    }

    let params = QueryParams::parse(uri.query(), &ctx)?;
    let q = params.required(&ctx, "q")?.to_string();
    let is_streaming = match params.get("stream") {
        None => false,
        Some("1") => true,
        Some(_) => return Err(ctx.bad_request("stream must be 1 when present")),
    };

    if is_streaming {
        let req = crate::handlers::parse_search_request(&ctx, &uri, &["stream"])?;
        // A rejected pin answers with its real status (400
        // `unknown_engines`, like `/api/search/stream` and `/api/search`)
        // rather than a streaming shell that opens into an error frame.
        state
            .pipeline()
            .validate_pin(&req)
            .await
            .map_err(|e| crate::handlers::search_error(&ctx, &req, e))?;
        let rid = ctx.request_id.as_uuid().to_string();
        let page = Page {
            nav_active: "search",
            answer_available: state.answer().is_some(),
            q,
            has_results: true,
            show_empty: false,
            empty_status: String::new(),
            result_count: 0,
            badge: crate::strings::search::SEARCHING.to_string(),
            request_id: rid.clone(),
            short_request_id: short_id(&rid),
            results: Vec::new(),
            more_url: String::new(),
            style_css: STYLE_CSS.clone(),
            is_streaming: true,
            stream_url: stream_url(&params, &req),
            query_hash: CacheKey::from(&req).as_str().to_string(),
            stream_strings: stream_strings(),
            ask_url: ask_url(&state, &req.q),
            index_on_click: state.archive_index_on_click(),
            // Streaming pages render the trigger disabled; the `meta`
            // frame's merged order arms it and fills the context.
            assist: state.answer().is_some(),
            assist_context: "[]".to_string(),
            assist_strings: assist_strings(),
        };
        return Ok(Html(
            page.render()
                .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
        )
        .into_response());
    }

    let (req, resp) = search_inner(&state, &ctx, &uri).await?;
    let rid = resp.meta.request_id.to_string();
    let rows = result_rows(&req, &resp);
    let more_url = more_url(&resp, &params, &req);
    let is_hx = headers.get("hx-request").is_some();

    if is_hx {
        let partial = Results {
            results: rows,
            more_url,
            show_empty: true,
            empty_status: engine_statuses(&resp).join(" · "),
        };
        Ok(Html(
            partial
                .render()
                .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
        )
        .into_response())
    } else {
        let page = Page {
            nav_active: "search",
            answer_available: state.answer().is_some(),
            q,
            has_results: true,
            show_empty: true,
            empty_status: engine_statuses(&resp).join(" · "),
            result_count: resp.results.len(),
            badge: badge(&resp),
            request_id: rid.clone(),
            short_request_id: short_id(&rid),
            results: rows,
            more_url,
            style_css: STYLE_CSS.clone(),
            is_streaming: false,
            stream_url: String::new(),
            query_hash: CacheKey::from(&req).as_str().to_string(),
            stream_strings: stream_strings(),
            ask_url: ask_url(&state, &req.q),
            index_on_click: state.archive_index_on_click(),
            // An assist answer needs something on the page to ground in.
            assist: state.answer().is_some() && !resp.results.is_empty(),
            assist_context: assist_context(&resp),
            assist_strings: assist_strings(),
        };
        Ok(Html(
            page.render()
                .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
        )
        .into_response())
    }
}

// ---------------------------------------------------------------------------
// W2-05 `/api/search` HTML arm — the engines page's inline test fragment
// ---------------------------------------------------------------------------

/// The fragment `GET /api/search` answers under `Accept: text/html`: a
/// meta line (`N results · <ms> ms`, or the error class alone) plus the
/// same `results.html` partial the search page renders, so the inline
/// test anatomy matches the results page (screen spec `engines.md`).
#[derive(Template)]
#[template(path = "search_fragment.html")]
struct SearchFragment {
    meta_line: String,
    results: Vec<Row>,
    /// Kept for the `results.html` include; always empty here — an inline
    /// test stays on page one.
    more_url: String,
    /// Same include contract; an inline test never renders the empty state.
    show_empty: bool,
    empty_status: String,
}

/// `Accept: text/html` arm of `GET /api/search` (W2-05): the inline test
/// result an engines card swaps under its actions row. On success the
/// fragment is the `N results · <ms> ms` meta line (or the `no results`
/// class) plus the result list; on failure it is the engine's error class
/// alone, answered with a swap-friendly 200 for HTMX callers so htmx
/// drops it in place instead of logging a `responseError`.
pub(crate) async fn search_fragment(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    use crate::strings::engines as copy;

    match crate::handlers::search_inner_classed(state, ctx, uri).await {
        Ok((req, resp)) => {
            let meta_line = if resp.results.is_empty() {
                copy::TEST_NO_RESULTS.to_string()
            } else {
                copy::TEST_RESULTS
                    .replace("{n}", &resp.results.len().to_string())
                    .replace("{ms}", &resp.meta.elapsed_ms.to_string())
            };
            let frag = SearchFragment {
                meta_line,
                results: result_rows(&req, &resp),
                more_url: String::new(),
                show_empty: false,
                empty_status: String::new(),
            };
            Ok(Html(
                frag.render()
                    .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
            )
            .into_response())
        }
        Err((e, class)) => {
            // HTMX swaps 2xx fragments in place; a real 4xx/5xx would
            // skip the swap and console-error, so the page's own fetch
            // gets 200 while a direct `Accept: text/html` caller sees the
            // true status.
            let status = if is_htmx(headers) {
                StatusCode::OK
            } else {
                e.status()
            };
            let frag = SearchFragment {
                meta_line: class.to_string(),
                results: Vec::new(),
                more_url: String::new(),
                show_empty: false,
                empty_status: String::new(),
            };
            match frag.render() {
                Ok(html) => Ok((status, Html(html)).into_response()),
                Err(_) => Ok((status, Html(String::new())).into_response()),
            }
        }
    }
}

fn badge(resp: &SearchResponse) -> String {
    use crate::strings::search as s;
    let base = match &resp.meta.source {
        Source::Cache { stale: true, .. } => s::STALE_BADGE.to_string(),
        Source::Cache { age_s, ttl_s, .. } => s::CACHED_BADGE
            .replace("{age}", &age_s.to_string())
            .replace("{ttl}", &ttl_s.to_string()),
        Source::Network => s::LIVE_BADGE.replace("{ms}", &resp.meta.elapsed_ms.to_string()),
    };
    let statuses = engine_statuses(resp);
    if statuses.is_empty() {
        base
    } else {
        format!("{base} · {}", statuses.join(" · "))
    }
}

fn engine_statuses(resp: &SearchResponse) -> Vec<String> {
    use crate::strings::search as s;
    let mut statuses = Vec::new();
    for report in &resp.meta.engines_used {
        statuses.push(match &report.status {
            EngineStatus::Ok => report.engine.to_string(),
            EngineStatus::Failed(error) => s::ENGINE_FAILED
                .replace("{engine}", report.engine.as_str())
                .replace("{kind}", engine_error_kind(error)),
        });
    }
    statuses.extend(
        resp.meta
            .engines_skipped
            .iter()
            .map(|engine| s::ENGINE_SKIPPED.replace("{engine}", engine.as_str())),
    );
    statuses
}

fn engine_error_kind(error: &EngineError) -> &'static str {
    use crate::strings::search as s;
    match error {
        EngineError::RateLimited => s::ERR_RATE_LIMITED,
        EngineError::Blocked => s::ERR_BLOCKED,
        EngineError::Timeout => s::ERR_TIMEOUT,
        EngineError::Parse(_) => s::ERR_PARSE,
        EngineError::Transport(_) => s::ERR_TRANSPORT,
        EngineError::NoResults => s::ERR_NO_RESULTS,
    }
}

/// The `strings::search` copy the streaming page's inline JS interpolates,
/// serialized once into the page as `var S = {...}` so every user-visible
/// string lives in `crate::strings` (the i18n seam), not in the script.
fn stream_strings() -> String {
    use crate::strings::search as s;
    serde_json::to_string(&json!({
        "results": s::RESULTS,
        "no_results": s::NO_RESULTS,
        "waiting": s::WAITING,
        "complete": s::COMPLETE,
        "invalid_stream": s::INVALID_STREAM,
        "new_above": s::NEW_ABOVE,
        "live_badge": s::LIVE_BADGE,
        "cached": s::CACHED,
        "stale_badge": s::STALE_BADGE,
        "engine_failed": s::ENGINE_FAILED,
        "engine_skipped": s::ENGINE_SKIPPED,
        "err_rate_limited": s::ERR_RATE_LIMITED,
        "err_blocked": s::ERR_BLOCKED,
        "err_timeout": s::ERR_TIMEOUT,
        "err_parse": s::ERR_PARSE,
        "err_transport": s::ERR_TRANSPORT,
        "err_no_results": s::ERR_NO_RESULTS,
        "err_unknown": s::ERR_UNKNOWN,
    }))
    .expect("search strings serialize")
}

/// W4-03: the `/answer?q=` link the meta line shows when an answer
/// loop exists (`ai` compiled in and the provider client built at
/// startup); empty otherwise, and the template renders no link.
fn ask_url(state: &AppState, q: &str) -> String {
    if state.answer().is_some() {
        format!("/answer?q={}", urlencoding::encode(q))
    } else {
        String::new()
    }
}

/// W7-02: the top-K SERP rows serialized in the `AnswerSource` wire
/// shape (`{url,title,snippet,engine}`) — what the Assist button POSTs
/// as `context_results`. `AnswerLoop::stream_assist` re-caps at
/// `ASSIST_MAX_SOURCES` (10) server-side; matching it here keeps the
/// prompt's `[n]` indices aligned with the visible top rows.
fn assist_context(resp: &SearchResponse) -> String {
    let rows: Vec<serde_json::Value> = resp
        .results
        .iter()
        .take(10)
        .map(|r| {
            json!({
                "url": r.url.as_str(),
                "title": r.title,
                "snippet": r.snippet,
                "engine": r.engine.as_str(),
            })
        })
        .collect();
    serde_json::to_string(&rows).expect("assist context serializes")
}

/// The `strings::assist` copy the assist JS interpolates, serialized
/// into the page as `var AS = {...}` (the `stream_strings` convention).
fn assist_strings() -> String {
    use crate::strings::assist as s;
    serde_json::to_string(&json!({
        "stream_failed": s::STREAM_FAILED,
        "invalid_stream": s::INVALID_STREAM,
        "retry_after": s::RETRY_AFTER,
        // W7-03: grounded/confidence chips in the card meta row.
        "grounded": s::GROUNDED,
        "ungrounded": s::UNGROUNDED,
        "confidence": s::CONFIDENCE,
        "cached": s::CACHED,
    }))
    .expect("assist strings serialize")
}

fn stream_url(params: &QueryParams, req: &SearchRequest) -> String {
    let mut parts = vec![format!("q={}", urlencoding::encode(&req.q))];
    if req.page != 1 {
        parts.push(format!("page={}", req.page));
    }
    for key in ["lang", "time_range", "safesearch", "engines"] {
        if let Some(value) = params.get(key) {
            parts.push(format!("{key}={}", urlencoding::encode(value)));
        }
    }
    // `EventSource` cannot set `X-Cauce-Client`; the query-param fallback
    // keeps UI-originated streams out of the `api` dashboard bucket.
    parts.push("client=ui".to_string());
    format!("/api/search/stream?{}", parts.join("&"))
}

fn result_rows(req: &SearchRequest, resp: &SearchResponse) -> Vec<Row> {
    let query_hash = CacheKey::from(req);

    resp.results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let host = r.url.host_str().unwrap_or("").to_string();
            let url = r.url.as_str().to_string();
            let hx_vals = serde_json::to_string(&json!({
                "url": url,
                "query_hash": query_hash.as_str(),
                "position": i,
            }))
            .unwrap_or_default();
            let favicon = if host.is_empty() {
                String::new()
            } else {
                format!("https://icons.duckduckgo.com/ip3/{host}.ico")
            };
            Row {
                favicon,
                url,
                title: r.title.clone(),
                host,
                snippet: r.snippet.clone(),
                hx_vals,
            }
        })
        .collect::<Vec<_>>()
}

fn more_url(resp: &SearchResponse, params: &QueryParams, req: &SearchRequest) -> String {
    if resp.results.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    parts.push(format!("q={}", urlencoding::encode(&req.q)));
    parts.push(format!("page={}", req.page.saturating_add(1)));
    if let Some(lang) = params.get("lang") {
        parts.push(format!("lang={}", urlencoding::encode(lang)));
    }
    if let Some(time_range) = params.get("time_range") {
        parts.push(format!("time_range={}", urlencoding::encode(time_range)));
    }
    if let Some(safesearch) = params.get("safesearch") {
        parts.push(format!("safesearch={}", urlencoding::encode(safesearch)));
    }
    if let Some(engines) = params.get("engines") {
        parts.push(format!("engines={}", urlencoding::encode(engines)));
    }
    format!("/search?{}", parts.join("&"))
}
