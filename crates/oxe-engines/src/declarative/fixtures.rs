//! `oxe engine test` fixture machinery (W1-02).
//!
//! A fixture pair under `engines/fixtures/<id>/` is a raw response body
//! (`<name>.html` for `parse.kind: html`, `<name>.json` for
//! `parse.kind: json`) plus `<name>.expected.json`:
//!
//! ```json
//! {
//!   "query": "tanstack router docs",
//!   "lang": "en",
//!   "page": 1,
//!   "status": 200,
//!   "results": [{ "title": "...", "url": "...", "snippet": "..." }]
//! }
//! ```
//!
//! `results` diffs field-by-field (title/url/snippet, order included).
//! `"error": "<variant>"` (`blocked`, `rate_limited`, `timeout`,
//! `no_results`, `parse`, `transport`) instead asserts the parse fails
//! with that `EngineError` variant — how captcha pages are pinned.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use oxe_core::{ClientKind, EngineError, SafeSearch, SearchRequest, SearchResult, normalize_query};

use super::spec::{CompiledSpec, SpecError};
use crate::cassette::cassette_key;

/// The `.expected.json` side of a fixture pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFixture {
    /// Query the body was recorded for (drives request templating and the
    /// base URL for relative links).
    pub query: String,
    /// Request language for templating (`{lang}`); `en` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Request page for templating; 1 when absent.
    #[serde(default = "default_page")]
    pub page: u8,
    /// Recorded HTTP status; 200 when absent.
    #[serde(default = "default_status")]
    pub status: u16,
    /// Expected parse output; mutually exclusive with `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ExpectedResult>>,
    /// Expected `EngineError` variant name (`blocked`, `rate_limited`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_page() -> u8 {
    1
}

fn default_status() -> u16 {
    200
}

/// One expected result row; `url` compares as its normalized string form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedResult {
    /// Expected `title` (whitespace-collapsed form).
    pub title: String,
    /// Expected normalized URL string.
    pub url: String,
    /// Expected `snippet`.
    #[serde(default)]
    pub snippet: String,
}

/// Why fixture IO or the diff failed.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// Filesystem failure.
    #[error("{}: {source}", .path.display())]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The `.expected.json` did not parse.
    #[error("{}: invalid expected json: {source}", .path.display())]
    Json {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        source: serde_json::Error,
    },
    /// The spec's `request.url` did not render for the fixture's request.
    #[error("cannot render request url for fixture: {0}")]
    Render(#[from] EngineError),
}

/// One fixture pair: body file plus its `.expected.json` sibling.
#[derive(Debug)]
pub struct FixturePair {
    /// Pair name (`<slug>-<sha8>`).
    pub name: String,
    /// Raw response body file.
    pub body_path: PathBuf,
    /// Expected-parse file.
    pub expected_path: PathBuf,
}

/// Outcome of running one pair.
#[derive(Debug)]
pub struct FixtureReport {
    /// Pair name.
    pub name: String,
    /// `Ok(result_count)` on pass, `Err(reason)` on fail.
    pub outcome: Result<usize, String>,
}

/// Every fixture pair under `<fixtures_root>/<id>/`, sorted by name.
/// Body files are `*.html`/`*.json`; `*.expected.json` files are never
/// bodies. A body without its expected sibling fails loudly.
pub fn fixture_pairs(fixtures_root: &Path, id: &str) -> Result<Vec<FixturePair>, FixtureError> {
    let dir = fixtures_root.join(id);
    let mut bodies: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                !n.ends_with(".expected.json") && (n.ends_with(".html") || n.ends_with(".json"))
            })
        })
        .collect();
    bodies.sort_unstable();

    let mut pairs = Vec::with_capacity(bodies.len());
    for body_path in bodies {
        let name = body_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let expected_path = dir.join(format!("{name}.expected.json"));
        if !expected_path.is_file() {
            return Err(FixtureError::Io {
                path: expected_path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("body {} has no .expected.json sibling", body_path.display()),
                ),
            });
        }
        pairs.push(FixturePair {
            name,
            body_path,
            expected_path,
        });
    }
    Ok(pairs)
}

/// Run one pair against `spec`: parse the body and diff against the
/// expected results (or expected error).
pub fn run_pair(spec: &CompiledSpec, pair: &FixturePair) -> Result<FixtureReport, FixtureError> {
    let expected_text =
        std::fs::read_to_string(&pair.expected_path).map_err(|source| FixtureError::Io {
            path: pair.expected_path.clone(),
            source,
        })?;
    let expected: ExpectedFixture =
        serde_json::from_str(&expected_text).map_err(|source| FixtureError::Json {
            path: pair.expected_path.clone(),
            source,
        })?;
    // Bytes, not `read_to_string`: live parsing decodes with
    // `from_utf8_lossy`, so `--record` can write a fixture whose body is
    // not valid UTF-8 (latin-1 pages); a hard UTF-8 read here would make
    // that fixture un-runnable.
    let body = std::fs::read(&pair.body_path).map_err(|source| FixtureError::Io {
        path: pair.body_path.clone(),
        source,
    })?;

    let req = SearchRequest {
        q: expected.query.clone(),
        page: expected.page,
        lang: expected.lang.clone(),
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Cli,
    };
    // No fetch happened, so the rendered request URL is the resolution
    // base (what `res.url` would be for a redirect-free live fetch).
    let base = spec.render_url(&req)?;
    let outcome = match spec.parse_response(expected.status, &body, &base) {
        Ok(results) => match (&expected.error, &expected.results) {
            (Some(err), _) => Err(format!(
                "expected error {err:?}, got {} results",
                results.len()
            )),
            (None, expected) => diff_results(expected.as_deref().unwrap_or(&[]), &results),
        },
        Err(err) => match &expected.error {
            Some(want) if want == error_name(&err) => Ok(0),
            Some(want) => Err(format!("expected error {want:?}, got {err:?} ({err})")),
            None => Err(format!("engine error: {err}")),
        },
    };
    Ok(FixtureReport {
        name: pair.name.clone(),
        outcome,
    })
}

/// `Vec<ExpectedResult>` vs parsed `SearchResult`s: exact diff over
/// title/url/snippet in order.
fn diff_results(expected: &[ExpectedResult], got: &[SearchResult]) -> Result<usize, String> {
    if expected.len() != got.len() {
        return Err(format!(
            "expected {} results, got {}",
            expected.len(),
            got.len()
        ));
    }
    for (i, (want, got)) in expected.iter().zip(got.iter()).enumerate() {
        let url = got.url.as_str();
        if want.title != got.title || want.url != url || want.snippet != got.snippet {
            return Err(format!(
                "result {i} differs: want {{title: {:?}, url: {:?}, snippet: {:?}}}, \
                 got {{title: {:?}, url: {:?}, snippet: {:?}}}",
                want.title, want.url, want.snippet, got.title, url, got.snippet,
            ));
        }
    }
    Ok(got.len())
}

/// `EngineError` variant name as used in `.expected.json` `"error"`.
pub fn error_name(err: &EngineError) -> &'static str {
    match err {
        EngineError::RateLimited => "rate_limited",
        EngineError::Blocked => "blocked",
        EngineError::Timeout => "timeout",
        EngineError::Parse(_) => "parse",
        EngineError::Transport(_) => "transport",
        EngineError::NoResults => "no_results",
    }
}

/// Filename base for a fixture pair: a readable slug of the normalized
/// query plus the cassette `sha8` so distinct queries never collide.
fn fixture_stem(query: &str) -> String {
    let slug: String = normalize_query(query)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "query".to_string()
    } else {
        slug.chars().take(40).collect()
    };
    format!("{slug}-{}", cassette_key(query))
}

/// Write a fixture pair under `<fixtures_root>/<id>/` (`oxe engine test
/// --live --record`). `body` is written verbatim; `ext` is `html` or
/// `json` matching `parse.kind`. Returns the body path.
pub fn write_pair(
    fixtures_root: &Path,
    spec: &CompiledSpec,
    query: &str,
    status: u16,
    body: &[u8],
    parsed: &Result<Vec<SearchResult>, EngineError>,
) -> Result<PathBuf, FixtureError> {
    let dir = fixtures_root.join(spec.id().as_str());
    std::fs::create_dir_all(&dir).map_err(|source| FixtureError::Io {
        path: dir.clone(),
        source,
    })?;

    let ext = match spec.spec().parse.kind {
        super::spec::ParseKind::Html => "html",
        super::spec::ParseKind::Json => "json",
    };
    let stem = fixture_stem(query);
    let body_path = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&body_path, body).map_err(|source| FixtureError::Io {
        path: body_path.clone(),
        source,
    })?;

    let expected = ExpectedFixture {
        query: normalize_query(query),
        lang: None,
        page: 1,
        status,
        results: match parsed {
            Ok(results) => Some(
                results
                    .iter()
                    .map(|r| ExpectedResult {
                        title: r.title.clone(),
                        url: r.url.as_str().to_string(),
                        snippet: r.snippet.clone(),
                    })
                    .collect(),
            ),
            Err(_) => None,
        },
        error: parsed.as_ref().err().map(error_name).map(str::to_string),
    };
    let json = serde_json::to_string_pretty(&expected).map_err(|source| FixtureError::Json {
        path: dir.join(format!("{stem}.expected.json")),
        source,
    })?;
    let expected_path = dir.join(format!("{stem}.expected.json"));
    std::fs::write(&expected_path, format!("{json}\n")).map_err(|source| FixtureError::Io {
        path: expected_path,
        source,
    })?;
    Ok(body_path)
}

/// Load+compile a spec for `oxe engine test`: a literal file path first,
/// then the `$config_dir/engines/` + embedded lookup by name (so
/// `oxe engine test bing` works once the spec ships).
pub fn compile_spec_source(
    name_or_path: &Path,
    config_dir: &Path,
    env: &oxe_core::config::EnvMap,
) -> Result<CompiledSpec, SpecError> {
    let name = name_or_path.to_string_lossy();
    let text = if name_or_path.is_file() {
        std::fs::read_to_string(name_or_path).map_err(|source| SpecError::Io {
            path: name_or_path.to_path_buf(),
            source,
        })?
    } else {
        super::loading::resolve_named(&name, config_dir)
            .ok_or_else(|| SpecError::NotFound(name.into_owned()))?
    };
    CompiledSpec::from_yaml(&text, env)
}
