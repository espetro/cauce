//! Frozen Exa wire shape (v2-legacy `oxe/api/exa.py` + `oxe/api/mcp.py`).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde::Serialize;

use super::*;

/// `highlights` per Exa item, frozen from `v2-legacy` (`_MAX_HIGHLIGHTS`,
/// `_HIGHLIGHT_SCORE`): first three sentences of the snippet, each scored 0.5.
const MAX_HIGHLIGHTS: usize = 3;
const HIGHLIGHT_SCORE: f32 = 0.5;

/// `results[]` item of the Exa response (frozen).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExaResultItem {
    title: String,
    url: String,
    /// Exa's opaque id; v2 used the URL.
    id: String,
    /// `contents.text` defaults to true in the v2 MCP tool, so the snippet.
    text: String,
    highlights: Vec<String>,
    highlight_scores: Vec<f32>,
    /// RFC 3339 when the engine reports a publication date.
    published_date: Option<String>,
    author: Option<String>,
    image: Option<String>,
    /// v2 sources this from `SearchResult.thumbnail` (NOT v1's Google
    /// `s2/favicons` URL — v2 dropped that computation). v3's
    /// `SearchResult` carries no thumbnail field, so this is always null
    /// until one lands.
    favicon: Option<String>,
    extras: ExaExtras,
}

#[derive(Debug, Clone, Serialize)]
struct ExaExtras {
    links: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaCostDollars {
    total: f64,
}

/// `exa_search` web/cache response (frozen `McpExaSearchResult`: Exa's
/// `ExaSearchResponse` plus the cauce-specific `tool_source`). The aliased
/// fields are camelCase; `tool_source` stays snake (no alias in v2).
#[derive(Debug, Serialize)]
pub(super) struct ExaSearchResult {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "searchType")]
    search_type: String,
    pub(super) results: Vec<ExaResultItem>,
    #[serde(rename = "costDollars")]
    cost_dollars: ExaCostDollars,
    tool_source: String,
}

/// `exa_search` `source = "history"` response (v2 `McpExaHistoryResult`,
/// plus `request_id` — v3's every-tool-result contract).
#[derive(Debug, Serialize)]
pub(super) struct ExaHistoryResult {
    pub(super) request_id: String,
    pub(super) query: String,
    pub(super) tool_source: &'static str,
    pub(super) results: Vec<ExaHistoryClick>,
}

/// One `clicks` row on the wire (v3 `ClickRow` fields, v2-compatible names).
#[derive(Debug, Serialize)]
pub(super) struct ExaHistoryClick {
    pub(super) id: Option<i64>,
    pub(super) query_hash: Option<String>,
    pub(super) url: String,
    pub(super) title: String,
    pub(super) position: u32,
    pub(super) clicked_at: String,
    pub(super) client: String,
}

/// v2 `_build_query_text`: `query` plus `-site:` operators for
/// `exclude_domains` (v3 has no `include_domains` arg — it is not in the
/// settled tool shape).
pub(super) fn build_query_text(query: &str, exclude_domains: Option<&[String]>) -> String {
    let mut parts = vec![query.trim().to_string()];
    if let Some(domains) = exclude_domains {
        parts.extend(
            domains
                .iter()
                .filter(|d| !d.is_empty())
                .map(|d| format!("-site:{d}")),
        );
    }
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// v2 `_extract_highlights`: first `_MAX_HIGHLIGHTS` sentences of the
/// snippet, each scored `_HIGHLIGHT_SCORE`.
pub(super) fn extract_highlights(snippet: &str) -> Vec<String> {
    split_sentences(snippet)
        .into_iter()
        .take(MAX_HIGHLIGHTS)
        .collect()
}

fn split_sentences(text: &str) -> Vec<String> {
    // The v2 regex `(?<=[.!?])\s+` without lookbehind: split on whitespace
    // that follows a sentence-final `.`, `!` or `?`.
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|n| n.is_whitespace()) {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    sentences
}

/// Canonical `SearchResponse` -> frozen Exa shape (v2
/// `searx_response_to_exa` + `_result_to_exa` with both `contents` flags on).
pub(super) fn exa_response(
    resp: &SearchResponse,
    args: &ExaSearchArgs,
    source: &str,
    num_results: u32,
    request_id: Uuid,
) -> ExaSearchResult {
    let results = resp
        .results
        .iter()
        .take(num_results as usize)
        .map(|r| {
            let url = r.url.to_string();
            let highlights = extract_highlights(&r.snippet);
            ExaResultItem {
                title: r.title.clone(),
                id: url.clone(),
                url,
                text: r.snippet.clone(),
                highlight_scores: vec![HIGHLIGHT_SCORE; highlights.len()],
                highlights,
                published_date: r.published.map(|d| d.to_rfc3339()),
                author: None,
                image: None,
                favicon: None,
                extras: ExaExtras { links: Vec::new() },
            }
        })
        .collect();
    ExaSearchResult {
        request_id: request_id.to_string(),
        search_type: args
            .search_type
            .clone()
            .unwrap_or_else(|| "auto".to_string()),
        results,
        cost_dollars: ExaCostDollars { total: 0.0 },
        tool_source: source.to_string(),
    }
}

