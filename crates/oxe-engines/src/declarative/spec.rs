//! YAML `EngineSpec` schema (parent plan section 4.3) and its validated,
//! compiled form.
//!
//! Raw schema (`serde_norway`, the maintained `serde_yaml` fork):
//!
//! ```yaml
//! id: brave
//! tier: 1                        # 1 fast/reliable, 2 hedge, 3 specialised
//! page_size: 10
//! enabled: true                  # auto-registration switch, default true
//! request:
//!   url: "https://search.brave.com/search?q={q}&offset={page0}"
//!   headers: { Accept-Language: "{lang}" }
//!   timeout_ms: 2500
//! parse:
//!   kind: html                   # html | json
//!   results: "div.snippet[data-type=web]"
//!   fields:
//!     title:   { css: ".title", text: true }
//!     url:     { css: "a", attr: href }
//!     snippet: { css: ".snippet-description", text: true }
//! detect:
//!   blocked: [ "captcha", "unusual traffic" ]
//!   rate_limited_status: [ 429, 403 ]
//! unwrap_redirect:
//!   - { match: "bing.com/ck/a", param: u, strip: "a1", base64: true }
//! ```
//!
//! `request.url` and `request.headers` values are templates: `{q}` (percent-
//! encoded query), `{page}` (1-based), `{page0}`, `{offset}` =
//! `(page-1) * page_size`, `{lang}` (request lang or `en`), plus `{name+N}`/
//! `{name-N}` arithmetic on the numeric names (Bing's `first={offset+1}`).
//! `{{`/`}}` are literal braces. Header values may additionally carry
//! `${env:NAME}`/`${file:PATH}` config interpolation, resolved at compile
//! time — before `{...}` templating — so secrets never sit in the spec.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::time::Duration;

use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json_path::JsonPath;
use url::Url;

use oxe_core::config::{ConfigError, EnvMap, interpolate_str};
use oxe_core::{EngineError, EngineId, SearchRequest, Tier};

use super::redirect::RedirectRule;

/// Why a spec failed to parse, interpolate or compile.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The YAML itself did not parse.
    #[error("invalid yaml: {0}")]
    Yaml(#[from] serde_norway::Error),
    /// A spec file could not be read.
    #[error("cannot read {}: {source}", .path.display())]
    Io {
        /// Path that failed.
        path: std::path::PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// `${env:...}`/`${file:...}` interpolation in `request.headers` failed.
    #[error("{0}")]
    Interpolate(#[from] ConfigError),
    /// The spec is inconsistent or references things that do not compile
    /// (bad CSS selector, bad JSONPath, unknown `{placeholder}`, ...).
    #[error("invalid spec {id:?}: {msg}")]
    Invalid {
        /// Spec id when known.
        id: String,
        /// What is wrong.
        msg: String,
    },
    /// The spec's `HttpClient` could not be built (bad proxy URL etc.).
    #[error("http client: {0}")]
    Http(#[from] EngineError),
    /// A named spec could not be resolved to any source.
    #[error("no spec named {0:?} (looked in $OXE_CONFIG_DIR/engines/ and embedded specs)")]
    NotFound(String),
}

/// Field names a spec may extract, in canonical order (`title`, `url`,
/// `snippet` map onto [`oxe_core::SearchResult`]).
pub const FIELD_NAMES: &[&str] = &["title", "url", "snippet"];

/// Default `page_size` when the spec omits it.
fn default_page_size() -> u8 {
    10
}

/// Default `tier` when the spec omits it (hedge tier, safest default).
fn default_tier() -> Tier {
    Tier::T2
}

fn default_enabled() -> bool {
    true
}

/// One parsed `engines/*.yaml` file (raw schema, not yet compiled).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineSpec {
    /// Stable engine id (`bing`, `brave`, `wikipedia`).
    pub id: EngineId,
    /// Fan-out tier: 1 fast/reliable, 2 hedge, 3 specialised.
    #[serde(default = "default_tier")]
    pub tier: Tier,
    /// Results per page the engine reports (drives `{offset}`).
    #[serde(default = "default_page_size")]
    pub page_size: u8,
    /// Auto-registration switch for specs without a `[[engines]]` entry.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Request template.
    pub request: RequestSpec,
    /// Response parsing.
    pub parse: ParseSpec,
    /// Block/rate-limit detection.
    #[serde(default)]
    pub detect: DetectSpec,
    /// Tracking-redirect unwrapping rules, tried in order.
    #[serde(default)]
    pub unwrap_redirect: Vec<RedirectRule>,
}

impl EngineSpec {
    /// Parse the YAML text into the raw schema (no validation yet).
    pub fn from_yaml(text: &str) -> Result<Self, SpecError> {
        Ok(serde_norway::from_str(text)?)
    }
}

/// `request:` section of a spec.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSpec {
    /// URL template (`{q}`, `{page}`, `{page0}`, `{offset}`, `{lang}`).
    pub url: String,
    /// Extra headers. Values run `${env:}`/`${file:}` interpolation at load
    /// and `{...}` templating per request.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Per-request timeout cap; `search` uses `min(budget, timeout_ms)`.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// `parse.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseKind {
    /// HTML body; `results` and field `css` are CSS selectors.
    Html,
    /// JSON body; `results` and field `path` are JSONPath expressions.
    Json,
}

/// `parse:` section of a spec.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParseSpec {
    /// Body format.
    pub kind: ParseKind,
    /// CSS selector (html) or JSONPath (json) selecting one node per result.
    /// For `kind: json` it must select an array — its length is the result
    /// count, which is what makes parallel-array APIs (OpenSearch) work.
    pub results: String,
    /// Per-field extraction. Only `title`, `url`, `snippet` are valid keys;
    /// `url` is required.
    #[serde(default)]
    pub fields: BTreeMap<String, FieldSpec>,
}

/// A YAML scalar or list of scalars (`css: "a"` / `css: ["a", "b"]`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    /// One value.
    One(String),
    /// A priority-ordered list (first non-empty match wins).
    Many(Vec<String>),
}

impl OneOrMany {
    fn items(&self) -> &[String] {
        match self {
            Self::One(s) => std::slice::from_ref(s),
            Self::Many(v) => v,
        }
    }
}

/// `parse.fields.<name>`: how one field is extracted from a result node.
///
/// HTML kind: `css` picks a descendant of the result element (a list is
/// tried in order; absent `css` means the element itself), then `attr`
/// reads an attribute or `text`/default reads the collapsed inner text.
///
/// JSON kind: `path` is a JSONPath evaluated against the result node, or
/// against the document root when `root: true`. `{i}` inside `path` is the
/// result index — needed for parallel-array APIs like OpenSearch, where
/// `url` is `$[3][{i}]` rather than a property of the node.
///
/// `regex` applies to the extracted string in both kinds: capture group 1
/// when present, else the whole match.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    /// HTML: descendant selector(s), priority order.
    #[serde(default)]
    pub css: Option<OneOrMany>,
    /// HTML: attribute to read instead of inner text.
    #[serde(default)]
    pub attr: Option<String>,
    /// HTML: explicit "inner text" marker (the default when `attr` is absent).
    #[serde(default)]
    pub text: Option<bool>,
    /// Post-extraction regex filter.
    #[serde(default)]
    pub regex: Option<String>,
    /// JSON: JSONPath to the field value (`{i}` = result index).
    #[serde(default)]
    pub path: Option<String>,
    /// JSON: evaluate `path` against the document root, not the node.
    #[serde(default)]
    pub root: Option<bool>,
}

/// `detect:` section: upstream block/rate-limit signals.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectSpec {
    /// Body substrings (matched case-insensitively) meaning the engine
    /// served a captcha/block page -> `EngineError::Blocked`.
    #[serde(default)]
    pub blocked: Vec<String>,
    /// HTTP statuses meaning the upstream is rate limiting ->
    /// `EngineError::RateLimited`.
    #[serde(default)]
    pub rate_limited_status: Vec<u16>,
}

/// Compiled `parse.results`: a CSS selector or a JSONPath.
#[derive(Debug)]
pub(crate) enum CompiledResults {
    Html(scraper::Selector),
    Json(JsonPath),
}

/// One field ready to extract (selectors, JSONPath and regex compiled).
#[derive(Debug)]
pub(crate) struct CompiledField {
    /// Canonical field name (`title` / `url` / `snippet`).
    pub name: &'static str,
    /// HTML: priority-ordered descendant selectors. Empty = result element.
    pub css: Vec<scraper::Selector>,
    /// HTML: attribute to read instead of inner text.
    pub attr: Option<String>,
    /// JSON: compiled `path` when it carries no `{i}` placeholder.
    pub path: Option<JsonPath>,
    /// JSON: raw `path` template when it carries `{i}` (compiled per result).
    pub path_template: Option<String>,
    /// JSON: evaluate against the document root.
    pub root: bool,
    /// Post-extraction filter (group 1 if any, else whole match).
    pub regex: Option<Regex>,
}

/// An [`EngineSpec`] after validation: every selector/JSONPath/regex is
/// compiled once, header `${...}` interpolation is done, and the URL
/// template is proven to render to an absolute URL.
#[derive(Debug)]
pub struct CompiledSpec {
    spec: EngineSpec,
    /// Header name + still-`{...}`-templated (but `${...}`-resolved) value.
    headers: Vec<(HeaderName, String)>,
    results: CompiledResults,
    /// Compiled fields in canonical order; missing `title`/`snippet` are
    /// absent (they default to `""`), `url` is always present.
    fields: Vec<CompiledField>,
    /// `detect.blocked` lowercased once for case-insensitive matching.
    blocked_lower: Vec<String>,
    /// `unwrap_redirect` rules with their `match` split into host/path.
    redirects: Vec<super::redirect::CompiledRedirect>,
}

impl CompiledSpec {
    /// Parse `text` as YAML, interpolate `${...}` in `request.headers`
    /// against `env`, then validate and compile.
    pub fn from_yaml(text: &str, env: &EnvMap) -> Result<Self, SpecError> {
        Self::compile(EngineSpec::from_yaml(text)?, env)
    }

    /// Validate and compile an already-parsed spec.
    pub fn compile(spec: EngineSpec, env: &EnvMap) -> Result<Self, SpecError> {
        let invalid = |msg: String| SpecError::Invalid {
            id: spec.id.to_string(),
            msg,
        };

        if spec.id.as_str().is_empty() {
            return Err(invalid("id must not be empty".to_string()));
        }
        if spec.page_size == 0 {
            return Err(invalid("page_size must be >= 1".to_string()));
        }

        validate_template(&spec.request.url, "request.url").map_err(invalid)?;
        // The URL must render to an absolute URL for a plausible request.
        let probe = render_template(
            &spec.request.url,
            &TemplateVars {
                q: "probe",
                page: 1,
                page_size: spec.page_size,
                lang: "en",
            },
        )
        .map_err(|e| invalid(e.to_string()))?;
        Url::parse(&probe).map_err(|e| {
            invalid(format!(
                "request.url renders to {probe:?} which is not an absolute url: {e}"
            ))
        })?;

        let mut headers = Vec::with_capacity(spec.request.headers.len());
        for (name, value) in &spec.request.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| invalid(format!("request.headers.{name:?}: bad name: {e}")))?;
            let interpolated =
                interpolate_str(value, env, &format!("request.headers.{}", name.as_str()))?;
            validate_template(&interpolated, &format!("request.headers.{name}"))
                .map_err(invalid)?;
            headers.push((name, interpolated));
        }

        let results = match spec.parse.kind {
            ParseKind::Html => CompiledResults::Html(
                scraper::Selector::parse(&spec.parse.results)
                    .map_err(|e| invalid(format!("parse.results css selector: {e}")))?,
            ),
            ParseKind::Json => CompiledResults::Json(
                JsonPath::parse(&spec.parse.results)
                    .map_err(|e| invalid(format!("parse.results jsonpath: {e}")))?,
            ),
        };

        if !spec.parse.fields.contains_key("url") {
            return Err(invalid(
                "parse.fields.url is required (a result without a url cannot be emitted)"
                    .to_string(),
            ));
        }
        let mut fields = Vec::new();
        for (name, fspec) in &spec.parse.fields {
            let Some(&canonical) = FIELD_NAMES.iter().find(|&&n| n == name.as_str()) else {
                return Err(invalid(format!(
                    "parse.fields.{name}: unknown field (valid: {})",
                    FIELD_NAMES.join(", ")
                )));
            };
            fields.push(compile_field(canonical, fspec, spec.parse.kind, &invalid)?);
        }
        // Emit in canonical order so fixtures/tests are deterministic.
        fields.sort_by_key(|f| FIELD_NAMES.iter().position(|n| *n == f.name));

        let blocked_lower = spec
            .detect
            .blocked
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        let redirects = spec
            .unwrap_redirect
            .iter()
            .map(|r| r.compile())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| invalid(e.to_string()))?;

        Ok(Self {
            spec,
            headers,
            results,
            fields,
            blocked_lower,
            redirects,
        })
    }

    /// The raw spec (id, tier, page_size, detect, ...).
    pub fn spec(&self) -> &EngineSpec {
        &self.spec
    }

    /// Spec engine id.
    pub fn id(&self) -> &EngineId {
        &self.spec.id
    }

    /// `min(budget, request.timeout_ms)` when the spec sets a timeout.
    pub fn effective_budget(&self, budget: Duration) -> Duration {
        match self.spec.request.timeout_ms {
            Some(ms) => budget.min(Duration::from_millis(ms)),
            None => budget,
        }
    }

    /// Render `request.url` for `req`. Errors are `EngineError::Transport`:
    /// a template that passed validation can only fail on a pathological
    /// request value, which is a request-side problem, not a parse problem.
    pub fn render_url(&self, req: &SearchRequest) -> Result<Url, EngineError> {
        let rendered = render_template(&self.spec.request.url, &self.vars(req))?;
        Url::parse(&rendered)
            .map_err(|e| EngineError::Transport(format!("rendered url {rendered:?} invalid: {e}")))
    }

    /// Render `request.headers` for `req` (`{lang}` etc. resolve per
    /// request; `${...}` secrets were resolved at compile).
    pub fn render_headers(&self, req: &SearchRequest) -> Result<HeaderMap, EngineError> {
        let vars = self.vars(req);
        let mut map = HeaderMap::with_capacity(self.headers.len());
        for (name, template) in &self.headers {
            let value = render_template(template, &vars)?;
            let value = HeaderValue::from_str(&value).map_err(|e| {
                EngineError::Transport(format!(
                    "header {} renders to an invalid value: {e}",
                    name.as_str()
                ))
            })?;
            map.insert(name.clone(), value);
        }
        Ok(map)
    }

    fn vars<'a>(&self, req: &'a SearchRequest) -> TemplateVars<'a> {
        TemplateVars {
            q: &req.q,
            page: req.page,
            page_size: self.spec.page_size,
            lang: req.lang.as_deref().unwrap_or("en"),
        }
    }

    /// `detect` + extraction: map `(status, body)` to results or the right
    /// `EngineError`. `base` resolves relative result urls — the final
    /// fetch URL live, the rendered request URL for fixtures.
    pub fn parse_response(
        &self,
        status: u16,
        body: &[u8],
        base: &Url,
    ) -> Result<Vec<oxe_core::SearchResult>, EngineError> {
        super::parse::parse_response(self, status, body, base)
    }

    /// Compiled `parse.results` (crate-internal).
    pub(crate) fn compiled_results(&self) -> &CompiledResults {
        &self.results
    }

    /// Compiled fields in canonical order (crate-internal).
    pub(crate) fn compiled_fields(&self) -> &[CompiledField] {
        &self.fields
    }

    /// Lowercased `detect.blocked` substrings (crate-internal).
    pub(crate) fn blocked_substrings(&self) -> &[String] {
        &self.blocked_lower
    }

    /// Compiled `unwrap_redirect` rules (crate-internal).
    pub(crate) fn redirects(&self) -> &[super::redirect::CompiledRedirect] {
        &self.redirects
    }
}

/// Compile one `parse.fields.<name>` entry for the spec's `parse.kind`.
fn compile_field(
    name: &'static str,
    f: &FieldSpec,
    kind: ParseKind,
    invalid: &dyn Fn(String) -> SpecError,
) -> Result<CompiledField, SpecError> {
    let mut out = CompiledField {
        name,
        css: Vec::new(),
        attr: None,
        path: None,
        path_template: None,
        root: f.root.unwrap_or(false),
        regex: f
            .regex
            .as_deref()
            .map(|r| Regex::new(r).map_err(|e| invalid(format!("parse.fields.{name}.regex: {e}"))))
            .transpose()?,
    };
    match kind {
        ParseKind::Html => {
            if f.path.is_some() || f.root.is_some() {
                return Err(invalid(format!(
                    "parse.fields.{name}: `path`/`root` are only valid for parse.kind: json"
                )));
            }
            if let Some(css) = &f.css {
                for sel in css.items() {
                    out.css.push(
                        scraper::Selector::parse(sel).map_err(|e| {
                            invalid(format!("parse.fields.{name}.css {sel:?}: {e}"))
                        })?,
                    );
                }
            }
            out.attr = f.attr.clone();
        }
        ParseKind::Json => {
            if f.css.is_some() || f.attr.is_some() || f.text.is_some() {
                return Err(invalid(format!(
                    "parse.fields.{name}: `css`/`attr`/`text` are only valid for parse.kind: html"
                )));
            }
            let Some(path) = &f.path else {
                return Err(invalid(format!(
                    "parse.fields.{name}: `path` is required for parse.kind: json"
                )));
            };
            if path.contains("{i}") {
                out.path_template = Some(path.clone());
            } else {
                out.path = Some(
                    JsonPath::parse(path)
                        .map_err(|e| invalid(format!("parse.fields.{name}.path {path:?}: {e}")))?,
                );
            }
        }
    }
    Ok(out)
}

/// Values substituted into `{...}` request templates.
pub(crate) struct TemplateVars<'a> {
    /// Raw query (percent-encoded by `{q}`).
    pub q: &'a str,
    /// 1-based page.
    pub page: u8,
    /// Spec `page_size` (drives `{offset}`).
    pub page_size: u8,
    /// Effective language (`req.lang` or `en`).
    pub lang: &'a str,
}

/// `{q}` encodes with the form-urlencoding byte serializer (space ->
/// `+`, the right flavor for query params), the numeric names take
/// `{name+N}`/`{name-N}` arithmetic clamped at 0, `{lang}` is verbatim.
/// `{{` and `}}` are literal-brace escapes; a lone `}` is copied
/// verbatim.
pub(crate) fn render_template(
    template: &str,
    vars: &TemplateVars<'_>,
) -> Result<String, EngineError> {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while !rest.is_empty() {
        // Copy literal text up to the next `{`, collapsing `}}` escapes
        // inside it (a lone `}` needs no escape and stays verbatim).
        let literal_end = rest.find('{').unwrap_or(rest.len());
        let mut literal = &rest[..literal_end];
        while let Some(pos) = literal.find("}}") {
            out.push_str(&literal[..pos + 1]);
            literal = &literal[pos + 2..];
        }
        out.push_str(literal);
        rest = &rest[literal_end..];
        if rest.is_empty() {
            break;
        }
        // `rest` starts with `{`.
        let after = &rest[1..];
        if let Some(tail) = after.strip_prefix('{') {
            out.push('{');
            rest = tail;
            continue;
        }
        let end = after.find('}').ok_or_else(|| {
            EngineError::Transport(format!("unterminated `{{` in template {template:?}"))
        })?;
        out.push_str(&resolve_token(&after[..end], vars)?);
        rest = &after[end + 1..];
    }
    Ok(out)
}

/// Substitute one `{...}` token. `name` may carry a `+N`/`-N` suffix,
/// valid only on the numeric names.
fn resolve_token(token: &str, vars: &TemplateVars<'_>) -> Result<String, EngineError> {
    let (name, delta) = match token.find(['+', '-']).map(|i| token.split_at(i)) {
        Some((name, delta)) => {
            let n: i64 = delta.parse().map_err(|_| {
                EngineError::Transport(format!("bad arithmetic in template token {{{token}}}"))
            })?;
            (name, n)
        }
        None => (token, 0),
    };
    let numeric = |v: i64| Ok(v.saturating_add(delta).max(0).to_string());
    match name {
        "q" if delta == 0 => Ok(url::form_urlencoded::byte_serialize(vars.q.as_bytes()).collect()),
        "lang" if delta == 0 => Ok(vars.lang.to_string()),
        "page" => numeric(i64::from(vars.page)),
        "page0" => numeric(i64::from(vars.page.saturating_sub(1))),
        "offset" => numeric(i64::from(vars.page.saturating_sub(1)) * i64::from(vars.page_size)),
        _ => Err(EngineError::Transport(format!(
            "unknown template token {{{token}}}"
        ))),
    }
}

/// Compile-time twin of [`render_template`]: catches unknown tokens and
/// unterminated braces when the spec loads instead of at search time.
fn validate_template(template: &str, where_: &str) -> Result<(), String> {
    let vars = TemplateVars {
        q: "x",
        page: 1,
        page_size: 10,
        lang: "en",
    };
    render_template(template, &vars).map_err(|e| format!("{where_}: {e}"))?;
    Ok(())
}
