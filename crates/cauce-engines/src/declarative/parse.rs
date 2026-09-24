//! Response parsing for declarative specs: `detect` mapping, then HTML
//! (`scraper`) or JSON (`serde_json_path`) extraction into
//! [`SearchResult`]s.
//!
//! Order of checks (a captcha page usually arrives with status 200):
//!
//! 1. `status` in `detect.rate_limited_status` -> `EngineError::RateLimited`
//! 2. extraction; when `parse.results` matches nothing and the body contains
//!    a `detect.blocked` substring -> `EngineError::Blocked`
//!    (case-insensitive). Engines echo the query into the body, so a marker
//!    only means "blocked" on a page that serves no results (#109).
//! 3. non-2xx status -> `EngineError::Transport("http <status>")`
//! 4. zero matched results -> `EngineError::Parse("0 results,
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

use cauce_core::{EngineError, SearchResult, normalize_url};

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

    let results = match spec.compiled_results() {
        CompiledResults::Html(sel) => Ok(parse_html(spec, sel, &text, base)),
        CompiledResults::Json(path) => parse_json(spec, path, &text, base),
    };
    let has_results = matches!(&results, Ok(r) if !r.is_empty());
    if !has_results && !spec.blocked_substrings().is_empty() {
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

    let mut results = results?;
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
/// Only `http`/`https` survive — a crafted `javascript:`/`data:`/`file:`
/// href (or a `u=` payload decoding to one) must never reach a
/// `SearchResult.url` that W2 renders as `<a href>`.
fn resolve_url(raw: &str, base: &Url, spec: &CompiledSpec) -> Option<Url> {
    let url = base.join(raw.trim()).ok()?;
    let url = unwrap_redirect(&url, spec.redirects());
    matches!(url.scheme(), "http" | "https").then(|| normalize_url(&url))
}

/// Collapse all whitespace runs (newlines/indentation inside scraped
/// text) into single spaces.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use proptest::prelude::*;
    use regex::Regex;

    use super::*;

    /// A spec exercising both redirect extract shapes; the `url` field is
    /// the only one `parse` requires.
    fn spec() -> &'static CompiledSpec {
        static SPEC: OnceLock<CompiledSpec> = OnceLock::new();
        SPEC.get_or_init(|| {
            CompiledSpec::from_yaml(
                r#"
id: fuzz
request: { url: "https://example.com/s?q={q}" }
parse:
  kind: json
  results: "$.items[*]"
  fields: { url: { path: "$.url" }, title: { path: "$.t" } }
unwrap_redirect:
  - { match: "bing.com/ck/a", param: u, strip: "a1", base64: true }
  - { match: "example.com", between: { start: "/RU=", end: "/" } }
"#,
                &BTreeMap::new(),
            )
            .unwrap()
        })
    }

    fn arb_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            any::<f64>()
                .prop_filter("finite", |f| f.is_finite())
                .prop_map(|f| Value::Number(serde_json::Number::from_f64(f).unwrap())),
            ".*".prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 32, 8, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::btree_map("[a-zA-Z0-9_.]{0,6}", inner, 0..6)
                    .prop_map(|m| Value::Object(m.into_iter().collect())),
            ]
        })
    }

    /// Url-shaped strings across schemes — including non-http(s) ones a
    /// hostile result href might carry (`javascript:`, `data:`, `file:`).
    fn arb_url_string() -> impl Strategy<Value = String> {
        (
            prop::sample::select(vec![
                "http",
                "https",
                "ftp",
                "javascript",
                "data",
                "file",
                "gopher",
            ]),
            "[a-z][a-z0-9]{0,9}(\\.[a-z0-9]{1,7}){0,2}",
            prop::collection::vec("[a-z0-9/_=.%-]{0,10}", 0..4),
            prop::collection::vec(("[a-z]{1,4}", "[a-zA-Z0-9%=&]{0,12}"), 0..3),
        )
            .prop_map(|(scheme, host, segs, pairs)| {
                let mut s = format!("{scheme}://{host}");
                for seg in segs {
                    s.push('/');
                    s.push_str(&seg);
                }
                if !pairs.is_empty() {
                    s.push('?');
                    s.push_str(
                        &pairs
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join("&"),
                    );
                }
                s
            })
    }

    fn arb_base() -> impl Strategy<Value = Url> {
        arb_url_string().prop_filter_map("base must be http(s)", |s| {
            Url::parse(&s)
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https"))
        })
    }

    /// Raw result hrefs: arbitrary junk, url-shaped strings, bing-style
    /// `u=a1<base64>` wrappers, and yahoo-style percent-encoded `/RU=`
    /// segments — the two redirect extract shapes under fuzz.
    fn arb_raw_result_url() -> impl Strategy<Value = String> {
        prop_oneof![
            4 => ".*",
            3 => arb_url_string(),
            2 => arb_url_string().prop_map(|inner| {
                format!("https://www.bing.com/ck/a?u=a1{}", URL_SAFE_NO_PAD.encode(inner))
            }),
            1 => prop::collection::vec(any::<u8>(), 0..32).prop_map(|b| {
                format!(
                    "https://example.com/x/RU={}/RK=1",
                    percent_encoding::percent_encode(&b, percent_encoding::NON_ALPHANUMERIC)
                )
            }),
        ]
    }

    fn arb_field() -> impl Strategy<Value = CompiledField> {
        let fixed = prop::sample::select(vec![
            "$",
            "$.a",
            "$.a.b",
            "$[0]",
            "$[0][1]",
            "$.items[*].v",
            "$..x",
        ]);
        let templated = prop::sample::select(vec!["$[{i}]", "$.a[{i}]", "$[2][{i}]"]);
        (
            prop::sample::select(vec!["title", "url", "snippet"]),
            prop_oneof![
                fixed.prop_map(|p| (Some(JsonPath::parse(p).unwrap()), None)),
                templated.prop_map(|t| (None, Some(t.to_string()))),
            ],
            any::<bool>(),
            prop::option::of(prop::sample::select(vec![".*", "\\d+", "^(a|b)$", "(.).*"])),
        )
            .prop_map(|(name, (path, path_template), root, re)| CompiledField {
                name,
                css: Vec::new(),
                attr: None,
                path,
                path_template,
                root,
                regex: re.map(|r| Regex::new(r).unwrap()),
            })
    }

    proptest! {
        /// Idempotent, never panics, and leaves no consecutive or
        /// leading/trailing whitespace.
        #[test]
        fn collapse_ws_idempotent_no_consecutive_ws(s in ".*") {
            let out = collapse_ws(&s);
            prop_assert!(out.chars().all(|c| !c.is_whitespace() || c == ' '));
            prop_assert!(!out.contains("  "));
            prop_assert_eq!(out.trim(), out.as_str());
            prop_assert_eq!(collapse_ws(&out), out);
        }

        /// `json_string` never panics and yields a string exactly for the
        /// scalar variants (string, number, bool).
        #[test]
        fn json_string_only_scalars(v in arb_json()) {
            let out = json_string(&v);
            match &v {
                Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                    prop_assert!(out.is_some());
                }
                _ => prop_assert!(out.is_none()),
            }
        }

        /// JSON field extraction on arbitrary docs/nodes/indexes never
        /// panics; output is a string or None.
        #[test]
        fn field_value_json_never_panics(
            doc in arb_json(),
            node in arb_json(),
            index in 0usize..16,
            fields in prop::collection::vec(arb_field(), 1..4),
            name in prop::sample::select(vec!["title", "url", "snippet"]),
        ) {
            let _ = field_value_json(&doc, &node, index, &fields, name);
        }

        /// The regex post-filter never panics on arbitrary input.
        #[test]
        fn apply_regex_never_panics(
            raw in ".*",
            re in prop::option::of(prop::sample::select(vec![".*", "\\d+", "(.)", "x{0,3}"])),
        ) {
            let f = CompiledField {
                name: "url",
                css: Vec::new(),
                attr: None,
                path: None,
                path_template: None,
                root: false,
                regex: re.map(|r| Regex::new(r).unwrap()),
            };
            let _ = apply_regex(&raw, &f);
        }

        /// SSRF-adjacent boundary: a produced result url is always
        /// http(s); anything else (including a `u=` payload decoding to
        /// `javascript:`/`data:`/`file:`) is declined.
        #[test]
        fn resolve_url_produces_http_or_declines(
            raw in arb_raw_result_url(),
            base in arb_base(),
        ) {
            if let Some(url) = resolve_url(&raw, &base, spec()) {
                prop_assert!(
                    matches!(url.scheme(), "http" | "https"),
                    "produced non-http(s) url {url}"
                );
            }
        }

        /// The full `(status, body)` boundary never panics.
        #[test]
        fn parse_response_never_panics(
            status in any::<u16>(),
            body in prop_oneof![
                prop::collection::vec(any::<u8>(), 0..256),
                arb_json().prop_map(|v| v.to_string().into_bytes()),
            ],
            base in arb_base(),
        ) {
            let _ = spec().parse_response(status, &body, &base);
        }
    }
}
