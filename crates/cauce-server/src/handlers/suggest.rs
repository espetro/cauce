//! `GET /api/suggest` — the OpenSearch suggestions endpoint the W2-11
//! descriptor advertises (`application/x-suggestions+json` Url, #150).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::QueryParams;
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// Completion cap the descriptor's consumers see (#150).
const SUGGEST_LIMIT: u32 = 10;

/// `GET /api/suggest?q=<term>`: the OpenSearch 1.1 suggestions shape
/// `[term, [completions...]]`, completions sourced from `search_log` by
/// `Store::suggest` — prefix-matched, frecency-ranked, capped at
/// [`SUGGEST_LIMIT`]. The store normalises the prefix the same way
/// `search_log.query` was written (`normalize_query`); the raw term
/// echoes back untouched. An absent or blank `q` answers `["", []]`;
/// the wire shape is always the two-element array.
pub async fn suggest(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["q"])?;
    let term = params.get("q").unwrap_or_default();
    let (term, completions) = if term.trim().is_empty() {
        ("", Vec::new())
    } else {
        (
            term,
            state
                .store()
                .suggest(term, SUGGEST_LIMIT)
                .await
                .map_err(|e| ctx.store(&e))?,
        )
    };
    let body = json!([term, completions]);
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/x-suggestions+json")],
        body.to_string(),
    )
        .into_response())
}
