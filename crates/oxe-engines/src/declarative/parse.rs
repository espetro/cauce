//! Response parsing for declarative specs: `detect` mapping, then HTML
//! (`scraper`) or JSON (`serde_json_path`) extraction into
//! [`SearchResult`]s.
//!
//! Order of checks (a captcha page usually arrives with status 200):
//!
//! 1. `status` in `detect.rate_limited_status` -> `EngineError::RateLimited`
//! 2. body contains a `detect.blocked` substring -> `EngineError::Blocked`
//!    (case-insensitive)
//! 3. non-2xx status -> `EngineError::Transport("http <status>")`
//! 4. extraction; zero matched results -> `EngineError::Parse("0 results,
//!    selector ...")` — an empty page means the layout drifted, not that
//!    the query has no answers (settled acceptance shape).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use scraper::{ElementRef, Html};
use serde_json::Value;
use serde_json_path::JsonPath;
use url::Url;

use oxe_core::{EngineError, SearchResult, normalize_url};

use super::redirect::unwrap_redirect;
use super::spec::{CompiledField, CompiledResults, CompiledSpec};

/// Map `(status, body)` to results or the right `EngineError`.
/// `base` resolves relative result urls (final fetch URL live, rendered
/// request URL for fixtures).
pub(crate) fn parse_response(
    spec: &CompiledSpec,
    status: u16,
    body: &[u8],
    base: &Url,
) -> Result<Vec<SearchResult>, EngineError> {
    if spec.spec().detect.rate_limited_status.contains(&status) {
        return Err(EngineError::RateLimited);
    }
    let text = String::from_utf8_lossy(body);
    if !spec.blocked_substrings().is_empty() {
        let lower = text.to_lowercase();
        if spec
            .blocked_substrings()
            .iter()
            .any(|needle| lower.contains(needle.as_str()))
        {
            return Err(EngineError::Blocked);
        }
    }
    if !(200..300).contains(&status) {
        return Err(EngineError::Transport(format!("http {status}")));
    }

    let mut results = match spec.compiled_results() {
        CompiledResults::Html(sel) => parse_html(spec, sel, &text, base),
        CompiledResults::Json(path) => parse_json(spec, path, &text, base)?,
    };
    if results.is_empty() {
        return Err(EngineError::Parse(format!(
            "0 results, selector {:?} matched nothing",
            spec.spec().parse.results
        )));
    }
    // Positional score: merge/RRF replaces it downstream.
    for (i, r) in results.iter_mut().enumerate() {
        r.score = 1.0 / (i + 1) as f32;
    }
    Ok(results)
}

/// `kind: html`: each `results` element yields one `SearchResult`.
fn parse_html(
    spec: &CompiledSpec,
    results_sel: &scraper::Selector,
    text: &str,
    base: &Url,
) -> Vec<SearchResult> {
    let doc = Html::parse_document(text);
    let mut out = Vec::new();
    for el in doc.select(results_sel) {
        let Some(url) = field_value_html(el, spec.compiled_fields(), "url")
            .and_then(|raw| resolve_url(&raw, base, spec))
        else {
            continue; // no usable url: the element is not a real result
        };
        let title = field_value_html(el, spec.compiled_fields(), "title").unwrap_or_default();
        let snippet = field_value_html(el, spec.compiled_fields(), "snippet").unwrap_or_default();
        out.push(SearchResult {
            url,
            title,
            snippet,
            engine: spec.id().clone(),
            published: None,
            score: 0.0,
        });
    }
    out
}

/// `kind: json`: `results` is a JSONPath whose located nodes are the
/// results; each field's `path` runs against the node (`root: false`) or
/// the document root with `{i}` substituted (`root: true`, for
/// parallel-array APIs like OpenSearch).
fn parse_json(
    spec: &CompiledSpec,
    results_path: &JsonPath,
    text: &str,
    base: &Url,
) -> Result<Vec<SearchResult>, EngineError> {
    let doc: Value = serde_json::from_str(text)
        .map_err(|e| EngineError::Parse(format!("body is not json: {e}")))?;
    let nodes: Vec<&Value> = results_path.query(&doc).all();
    // A lone array node means the spec pointed at the array itself
    // (`$[1]` in an OpenSearch response) — iterate its elements so
    // parallel-array APIs work without a `[*]` suffix.
    let nodes: Vec<&Value> = match nodes.as_slice() {
        [Value::Array(items)] => items.iter().collect(),
        _ => nodes,
    };
    let mut out = Vec::new();
    for (i, &node) in nodes.iter().enumerate() {
        let get = |name: &str| field_value_json(&doc, node, i, spec.compiled_fields(), name);
        let Some(url) = get("url").and_then(|raw| resolve_url(&raw, base, spec)) else {
            continue;
        };
        out.push(SearchResult {
            url,
            title: get("title").unwrap_or_default(),
            snippet: get("snippet").unwrap_or_default(),
            engine: spec.id().clone(),
            published: None,
            score: 0.0,
        });
    }
    Ok(out)
}

/// Extract one field from an HTML result element: pick the first `css`
/// selector with a match (priority order; absent `css` = the element
/// itself), read `attr` or the collapsed inner text, apply `regex`.
fn field_value_html(el: ElementRef<'_>, fields: &[CompiledField], name: &str) -> Option<String> {
    let f = fields.iter().find(|f| f.name == name)?;
    let target = f
        .css
        .iter()
        .find_map(|sel| el.select(sel).next())
        .unwrap_or(el);
    let raw = match &f.attr {
        Some(attr) => target.value().attr(attr).map(str::to_string),
        None => Some(collapse_ws(&target.text().collect::<String>())),
    }?;
    apply_regex(&raw, f)
}

/// Extract one field from a JSON result node (`{i}` = result index).
fn field_value_json(
    doc: &Value,
    node: &Value,
    index: usize,
    fields: &[CompiledField],
    name: &str,
) -> Option<String> {
    let f = fields.iter().find(|f| f.name == name)?;
    let path = match (&f.path, &f.path_template) {
        (Some(p), _) => p.clone(),
        (None, Some(t)) => JsonPath::parse(&t.replace("{i}", &index.to_string())).ok()?,
        (None, None) => return None,
    };
    let root = if f.root { doc } else { node };
    let raw = json_string(path.query(root).first()?)?;
    apply_regex(&raw, f)
}

/// JSON scalar to text; arrays/objects/null yield nothing.
fn json_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(v.to_string()),
        _ => None,
    }
}

/// `regex` post-filter: capture group 1 when present, else the whole
/// match. No match at all -> no value.
fn apply_regex(raw: &str, f: &CompiledField) -> Option<String> {
    let Some(re) = &f.regex else {
        return Some(raw.to_string());
    };
    let caps = re.captures(raw)?;
    let m = caps.get(1).or_else(|| caps.get(0))?;
    Some(m.as_str().to_string())
}

/// Unwrap tracking redirects, resolve against `base`, normalize.
fn resolve_url(raw: &str, base: &Url, spec: &CompiledSpec) -> Option<Url> {
    let url = base.join(raw.trim()).ok()?;
    Some(normalize_url(&unwrap_redirect(&url, spec.redirects())))
}

/// Collapse all whitespace runs (newlines/indentation inside scraped
/// text) into single spaces.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
