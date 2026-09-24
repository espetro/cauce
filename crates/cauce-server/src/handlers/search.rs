//! `GET /api/search` and the SSE `GET /api/search/stream`, plus the shared
//! search execution and error mapping the HTML fragments reuse.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::convert::Infallible;

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{
    IntoResponse, Json, Response, Sse,
    sse::{Event, KeepAlive},
};
use cauce_core::{
    ClientKind, EngineId, PipelineError, SafeSearch, SearchOpts, SearchRequest, SearchResponse,
    SearchResult, StreamEvent, TimeRange,
};
use serde_json::{Value, json};
use tokio_stream::StreamExt;

use super::{QueryParams, engine_error_class, search_error_payload};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// `GET /api/search?q&page&lang&time_range&safesearch&engines`.
///
/// `engines` is a comma-separated pin (`engines=replay,ddgs`); a pin
/// naming any id outside the configured set is 400 `unknown_engines` —
/// the message lists the rejected ids and the configured set (issue #90
/// strict contract, even if no engines are configured) — an empty pin or
/// zero configured engines is 503 `no_engines`, and an all-failed fan-out
/// is 502 `upstream_failed`.
///
/// `Accept: text/html` renders the inline result-list fragment the
/// engines page's test query swaps in (W2-05) — same handler, negotiated.
#[cfg_attr(not(feature = "ui"), allow(unused_variables))]
pub async fn search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    #[cfg(feature = "ui")]
    if crate::html::accepts_html(&headers) {
        return crate::html::search_fragment(&state, &ctx, &uri, &headers).await;
    }
    search_inner(&state, &ctx, &uri)
        .await
        .map(|(_req, resp)| Json(resp).into_response())
}

/// Shared search execution used by `GET /api/search` and the HTML/HTMX page.
/// Returns the canonical [`SearchRequest`] alongside the response so callers
/// can compute the cache key and pagination URL without re-parsing params.
pub(crate) async fn search_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<(SearchRequest, SearchResponse), ApiError> {
    let req = parse_search_request(ctx, uri, &[])?;
    match state
        .pipeline()
        .search_with_id(&req, ctx.request_id.as_uuid())
        .await
    {
        Ok(resp) => Ok((req, resp)),
        Err(error) => Err(search_error(ctx, &req, error)),
    }
}

/// Parse the canonical search parameters. A page can opt into one additional
/// private query parameter (`stream`) without widening the JSON API contract.
pub(crate) fn parse_search_request(
    ctx: &RequestCtx,
    uri: &Uri,
    extra: &[&str],
) -> Result<SearchRequest, ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    let mut allowed = vec!["q", "page", "lang", "time_range", "safesearch", "engines"];
    allowed.extend_from_slice(extra);
    params.allow(ctx, &allowed)?;
    Ok(SearchRequest {
        q: params.required(ctx, "q")?.to_string(),
        page: params.page(ctx)?,
        lang: params.get("lang").map(str::to_string),
        time_range: params
            .get("time_range")
            .map(|v| v.parse::<TimeRange>().map_err(|e| ctx.bad_request(e)))
            .transpose()?,
        safesearch: params
            .get("safesearch")
            .map(|v| v.parse::<SafeSearch>().map_err(|e| ctx.bad_request(e)))
            .transpose()?
            .unwrap_or_default(),
        engines: params
            .get("engines")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(EngineId::from)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty()),
        client: ctx.client.clone(),
    })
}

/// `GET /api/search/stream?q=...`: `results` per engine completion,
/// followed by one terminal `meta` or `error` frame.
///
/// Two pre-stream rejections answer with their real status instead of an
/// SSE body: request-parameter errors (`parse_search_request`, 400
/// `bad_request`) and a rejected engine pin (400 `unknown_engines` /
/// 503 `no_engines`, mapped through [`search_error`] like `/api/search`).
/// Failures past that point (fan-out, admission, store) are terminal
/// `error` events on the open stream.
///
/// `client=ui|api|mcp[:<name>]|cli` is accepted as a client-kind hint when
/// the `X-Cauce-Client` header is absent: `EventSource` cannot set
/// headers, so the page passes `client=ui` to keep the dashboard's
/// ui/api split honest. Same trust level as the header (a caller-supplied
/// hint), so invalid values are ignored, not rejected.
pub async fn search_stream(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    let mut req = parse_search_request(&ctx, &uri, &["client"])?;
    if headers.get("x-cauce-client").is_none()
        && let Some(client) = params.get("client").and_then(client_hint)
    {
        req.client = client;
    }
    let receiver = state
        .pipeline()
        .search_stream(
            &req,
            SearchOpts {
                request_id: Some(ctx.request_id.as_uuid()),
                ttl: None,
            },
        )
        .await
        .map_err(|error| search_error(&ctx, &req, error))?;
    let stream_ctx = ctx.clone();
    let stream_req = req.clone();
    let events = tokio_stream::wrappers::UnboundedReceiverStream::new(receiver).map(move |event| {
        let wire_event = match event {
            StreamEvent::Results {
                engine,
                results,
                elapsed_ms,
            } => Event::default()
                .event("results")
                .json_data(json!({
                    "engine": engine,
                    "results": results.iter().map(stream_result_json).collect::<Vec<_>>(),
                    "elapsed_ms": elapsed_ms,
                }))
                .expect("search result event serializes"),
            StreamEvent::Meta(meta) => Event::default()
                .event("meta")
                .json_data(meta)
                .expect("search meta event serializes"),
            StreamEvent::Error(error) => Event::default()
                .event("error")
                .json_data(search_error_payload(&stream_ctx, &stream_req, error))
                .expect("search error event serializes"),
        };
        Ok::<Event, Infallible>(wire_event)
    });
    Ok(Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// A `client` query-param value parsed like the `X-Cauce-Client` header
/// (`ui`, `api`, `cli`, `mcp` or `mcp:<name>`); anything else is `None`
/// and ignored by the caller.
fn client_hint(value: &str) -> Option<ClientKind> {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "ui" | "web" => return Some(ClientKind::Ui),
        "api" => return Some(ClientKind::Api),
        "cli" => return Some(ClientKind::Cli),
        "mcp" => return Some(ClientKind::Mcp("unknown".to_string())),
        _ => {}
    }
    lower.strip_prefix("mcp:").map(|name| {
        let name = name.trim();
        ClientKind::Mcp(if name.is_empty() {
            "unknown".to_string()
        } else {
            name.to_string()
        })
    })
}

/// A streamed result plus `key`, the server-side dedupe key
/// (`normalize_url` of its URL — the same form `meta.order` carries). The
/// progressive page dedupes appended articles on `key`: the merge drops
/// duplicate URL spellings the raw `url` field would render twice.
fn stream_result_json(result: &SearchResult) -> Value {
    let mut value = serde_json::to_value(result).expect("SearchResult serializes");
    value["key"] = json!(cauce_core::normalize_url(&result.url));
    value
}

pub(crate) fn search_error(
    ctx: &RequestCtx,
    req: &SearchRequest,
    error: PipelineError,
) -> ApiError {
    match error {
        error @ PipelineError::UnknownEngines { .. } => ctx.err(
            StatusCode::BAD_REQUEST,
            "unknown_engines",
            error.to_string(),
        ),
        PipelineError::NoEngines if req.engines.is_some() => ctx.err(
            StatusCode::BAD_REQUEST,
            "unknown_engines",
            "engines pin matched no configured engine",
        ),
        PipelineError::NoEngines => ctx.err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_engines",
            "no search engines configured",
        ),
        error @ PipelineError::AllEnginesFailed(_) => ctx.err(
            StatusCode::BAD_GATEWAY,
            "upstream_failed",
            error.to_string(),
        ),
        PipelineError::RateLimited { retry_after_s } => ctx
            .err(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("admission queue saturated; retry after {retry_after_s}s"),
            )
            .with_retry_after(retry_after_s),
        error @ PipelineError::BreakerOpen(_) => ctx.err(
            StatusCode::SERVICE_UNAVAILABLE,
            "breaker_open",
            error.to_string(),
        ),
    }
}

/// [`search_inner`] plus a display class on the error side (`blocked`,
/// `timeout`, `no results`, `breaker open`, ...) — the engines page's
/// inline test fragment renders it as the meta line on failure (screen
/// spec `engines.md`: "the engine's error class"). JSON callers discard
/// the class through [`search_inner`]. The dispatch below intentionally
/// mirrors `search_inner` (the error arm needs the classed mapping, not
/// `search_error` alone); only the `ui` build's fragment calls it.
#[cfg_attr(not(feature = "ui"), allow(dead_code))]
pub(crate) async fn search_inner_classed(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<(SearchRequest, SearchResponse), (ApiError, &'static str)> {
    use crate::strings::engines as copy;

    let req = parse_search_request(ctx, uri, &[]).map_err(|e| (e, copy::TEST_BAD_REQUEST))?;
    match state
        .pipeline()
        .search_with_id(&req, ctx.request_id.as_uuid())
        .await
    {
        Ok(resp) => Ok((req, resp)),
        Err(error) => Err(search_error_classed(ctx, &req, error)),
    }
}

/// [`search_error`] plus the display class the engines page's inline test
/// renders: the class mirrors the error arm, `upstream failed` or the
/// single engine's [`cauce_core::EngineError`] class for an all-failed
/// fan-out.
#[cfg_attr(not(feature = "ui"), allow(dead_code))]
pub(crate) fn search_error_classed(
    ctx: &RequestCtx,
    req: &SearchRequest,
    error: PipelineError,
) -> (ApiError, &'static str) {
    use crate::strings::engines as copy;

    let class = match &error {
        PipelineError::UnknownEngines { .. } => copy::TEST_UNKNOWN_ENGINES,
        PipelineError::NoEngines if req.engines.is_some() => copy::TEST_UNKNOWN_ENGINES,
        PipelineError::NoEngines => copy::TEST_NO_ENGINES,
        PipelineError::AllEnginesFailed(failures) => engine_error_class(failures),
        PipelineError::RateLimited { .. } => copy::TEST_RATE_LIMITED,
        PipelineError::BreakerOpen(_) => copy::TEST_BREAKER_OPEN,
    };
    (search_error(ctx, req, error), class)
}
