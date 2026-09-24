//! `cauce engine test --live` drift canary (W3-06): re-fetch the committed
//! fixture's recorded query and fail loudly when the parser degraded.
//!
//! Checks (any failure exits the canary non-zero):
//!
//! * page 1 fetch/parse error (including `Parse("0 results ...")` — a dead
//!   `parse.results` selector), or an empty result set;
//! * page-1 result count below `MIN_COUNT_RATIO` of the committed fixture
//!   (partial selector drift, SearXNG #4910);
//! * per-field fill rate (title/url/snippet non-empty share) below
//!   `MIN_FILL_RATE` of the fixture's — fields drifting empty while the
//!   container selector still matches;
//! * page-2 normalized-url overlap with page 1 above `MAX_URL_OVERLAP`
//!   (the engine silently serving page 1 again, SearXNG #3402/#4546).
//!
//! Page 2 is only fetched when the spec's `request.url` renders
//! differently for `page=2` — specs without a page placeholder
//! (OpenSearch-style) have no page-2 canary. A page-2 fetch/parse failure
//! is a note, not a failure: upstreams legitimately rate-limit or end
//! pagination, and the plan's failure list covers overlap only.
//!
//! `run` takes the fetch step injected so tests replay recorded bodies
//! through the same evaluation path the nightly drives with real HTTP.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::path::Path;

use cauce_core::{ClientKind, EngineError, SafeSearch, SearchRequest, SearchResult};

use super::Fetched;
use super::fixtures::{
    ExpectedFixture, ExpectedResult, FixtureError, fixture_pairs, load_expected,
};
use super::spec::CompiledSpec;

/// Page-1 live result count may not drop below this share of the
/// committed fixture's count.
pub const MIN_COUNT_RATIO: f64 = 0.5;
/// Per-field live fill rate may not drop below this share of the
/// fixture's fill rate.
pub const MIN_FILL_RATE: f64 = 0.5;
/// Page 2 may share at most this share of page-1 normalized urls before
/// the 'serves page 1 again' anti-bot pattern trips.
pub const MAX_URL_OVERLAP: f64 = 0.7;

/// One spec's canary outcome: empty `failures` means healthy.
#[derive(Debug, Default)]
pub struct CanaryReport {
    /// Hard failures — each is printed as its own `FAIL` line.
    pub failures: Vec<String>,
    /// Non-fatal observations (page-2 unavailable, overlap unchecked).
    pub notes: Vec<String>,
    /// Parsed page-1 result count when the fetch+parse succeeded.
    pub page1_count: usize,
    /// Parsed page-2 result count when the spec paginates and fetched.
    pub page2_count: Option<usize>,
}

impl CanaryReport {
    /// `true` when no check failed.
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// The request the baseline fixture was recorded with, on `page`.
fn request_for(baseline: &ExpectedFixture, page: u8) -> SearchRequest {
    SearchRequest {
        q: baseline.query.clone(),
        page,
        lang: baseline.lang.clone(),
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Cli,
    }
}

/// Whether `request.url` renders differently for page 2 — a spec without
/// a `{page}`/`{page0}`/`{offset}` placeholder has no page-2 canary.
pub fn paginates(spec: &CompiledSpec, baseline: &ExpectedFixture) -> bool {
    spec.render_url(&request_for(baseline, 1)).ok()
        != spec.render_url(&request_for(baseline, 2)).ok()
}

/// The canary's baseline fixture: the first pair (sorted by name) that
/// asserts a successful page-1 parse — `error` absent, `page` 1 and
/// `results` non-empty. `Ok(None)` means the spec has no usable committed
/// fixture and the canary cannot run.
pub fn baseline(
    fixtures_root: &Path,
    spec_id: &str,
) -> Result<Option<(String, ExpectedFixture)>, FixtureError> {
    for pair in fixture_pairs(fixtures_root, spec_id)? {
        let expected = load_expected(&pair.expected_path)?;
        if expected.error.is_none()
            && expected.page == 1
            && expected.results.as_ref().is_some_and(|r| !r.is_empty())
        {
            return Ok(Some((pair.name, expected)));
        }
    }
    Ok(None)
}

/// Fetch page 1 (and page 2 when [`paginates`]) through `fetch`, parse
/// with `spec` and evaluate the canary checks. `fetch` is injected: the
/// CLI wires `DeclarativeEngine::fetch`, tests feed recorded bodies.
pub fn run(
    spec: &CompiledSpec,
    baseline: &ExpectedFixture,
    fetch: &dyn Fn(&SearchRequest) -> Result<Fetched, EngineError>,
) -> CanaryReport {
    let page1 = fetch(&request_for(baseline, 1))
        .and_then(|f| spec.parse_response(f.status, &f.body, &f.url));
    let page2 = paginates(spec, baseline).then(|| {
        fetch(&request_for(baseline, 2))
            .and_then(|f| spec.parse_response(f.status, &f.body, &f.url))
    });
    evaluate(baseline, &page1, page2.as_ref())
}

/// Evaluate the checks against already-parsed pages; `page2` is `None`
/// when the spec does not paginate.
pub fn evaluate(
    baseline: &ExpectedFixture,
    page1: &Result<Vec<SearchResult>, EngineError>,
    page2: Option<&Result<Vec<SearchResult>, EngineError>>,
) -> CanaryReport {
    let mut report = CanaryReport::default();
    let fixture = baseline.results.as_deref().unwrap_or(&[]);

    let page1_results = match page1 {
        Err(e) => {
            report.failures.push(format!("page 1: {e}"));
            None
        }
        Ok(results) => {
            report.page1_count = results.len();
            if results.is_empty() {
                report
                    .failures
                    .push("page 1 returned 0 results".to_string());
                None
            } else {
                let floor = MIN_COUNT_RATIO * fixture.len() as f64;
                if (results.len() as f64) < floor {
                    report.failures.push(format!(
                        "page 1 yielded {} results, under {:.0}% of the fixture's {}",
                        results.len(),
                        MIN_COUNT_RATIO * 100.0,
                        fixture.len()
                    ));
                }
                check_fill_rates(results, fixture, &mut report);
                Some(results)
            }
        }
    };

    if let Some(p2) = page2 {
        match p2 {
            Err(e) => report
                .notes
                .push(format!("page 2 unavailable ({e}); overlap unchecked")),
            Ok(p2_results) => {
                report.page2_count = Some(p2_results.len());
                match (page1_results, p2_results.is_empty()) {
                    (Some(p1), false) => {
                        let overlap = url_overlap(p1, p2_results);
                        if overlap > MAX_URL_OVERLAP {
                            report.failures.push(format!(
                                "page 2 shares {:.0}% of page-1 urls (>{:.0}%: 'serves page 1 \
                                 again' anti-bot signal)",
                                overlap * 100.0,
                                MAX_URL_OVERLAP * 100.0
                            ));
                        }
                    }
                    (Some(_), true) => report
                        .notes
                        .push("page 2 returned 0 results; overlap unchecked".to_string()),
                    (None, _) => {}
                }
            }
        }
    }
    report
}

/// Per-field fill rate on live results versus the fixture: a field
/// populated in the fixture but gone live is partial selector drift.
fn check_fill_rates(live: &[SearchResult], fixture: &[ExpectedResult], report: &mut CanaryReport) {
    for field in ["title", "url", "snippet"] {
        let want = fill_rate(fixture, &|r| field_of_expected(r, field));
        if want == 0.0 {
            continue;
        }
        let got = fill_rate(live, &|r| field_of(r, field));
        if got < MIN_FILL_RATE * want {
            report.failures.push(format!(
                "{field} fill rate {:.0}% vs the fixture's {:.0}%",
                got * 100.0,
                want * 100.0
            ));
        }
    }
}

/// Share of `rows` whose `field` is non-empty.
fn fill_rate<T>(rows: &[T], field: &dyn Fn(&T) -> &str) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    rows.iter().filter(|r| !field(r).is_empty()).count() as f64 / rows.len() as f64
}

fn field_of<'a>(r: &'a SearchResult, field: &str) -> &'a str {
    match field {
        "title" => r.title.as_str(),
        "snippet" => r.snippet.as_str(),
        _ => r.url.as_str(),
    }
}

fn field_of_expected<'a>(r: &'a ExpectedResult, field: &str) -> &'a str {
    match field {
        "title" => r.title.as_str(),
        "snippet" => r.snippet.as_str(),
        _ => r.url.as_str(),
    }
}

/// `|p1 ∩ p2| / min(|p1|, |p2|)` over normalized url strings (urls are
/// already normalized by the parse path).
fn url_overlap(page1: &[SearchResult], page2: &[SearchResult]) -> f64 {
    let p1: HashSet<&str> = page1.iter().map(|r| r.url.as_str()).collect();
    let shared = page2.iter().filter(|r| p1.contains(r.url.as_str())).count();
    shared as f64 / page1.len().min(page2.len()) as f64
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use url::Url;

    use super::*;

    const SPEC_YAML: &str = r#"
id: fixture
request:
  url: "https://search.test.local/s?q={q}&first={offset+1}"
parse:
  kind: html
  results: "div.result"
  fields:
    title: { css: "h2.t", text: true }
    url: { css: "a.u", attr: href }
    snippet: { css: "p.s", text: true }
"#;

    const NOPAGE_SPEC_YAML: &str = r#"
id: nopage
request:
  url: "https://search.test.local/s?q={q}"
parse:
  kind: html
  results: "div.result"
  fields:
    title: { css: "h2.t", text: true }
    url: { css: "a.u", attr: href }
    snippet: { css: "p.s", text: true }
"#;

    fn spec() -> CompiledSpec {
        CompiledSpec::from_yaml(SPEC_YAML, &BTreeMap::new()).unwrap()
    }

    fn nopage_spec() -> CompiledSpec {
        CompiledSpec::from_yaml(NOPAGE_SPEC_YAML, &BTreeMap::new()).unwrap()
    }

    fn html(results: &[(&str, &str, &str)]) -> Vec<u8> {
        let mut body = String::from("<html><body>");
        for (title, url, snippet) in results {
            body.push_str(&format!(
                r#"<div class="result"><h2 class="t">{title}</h2><a class="u" href="{url}">l</a><p class="s">{snippet}</p></div>"#
            ));
        }
        body.push_str("</body></html>");
        body.into_bytes()
    }

    /// `n` results with every field populated, `pfx` namespacing the urls.
    fn rows(n: usize, pfx: &str) -> Vec<(String, String, String)> {
        (0..n)
            .map(|i| {
                (
                    format!("title {i}"),
                    format!("https://example.com/{pfx}{i}"),
                    format!("snippet {i}"),
                )
            })
            .collect()
    }

    fn baseline_fixture(n: usize) -> ExpectedFixture {
        ExpectedFixture {
            query: "canary query".to_string(),
            lang: None,
            page: 1,
            status: 200,
            results: Some(
                (0..n)
                    .map(|i| ExpectedResult {
                        title: format!("title {i}"),
                        url: format!("https://example.com/f{i}"),
                        snippet: format!("snippet {i}"),
                    })
                    .collect(),
            ),
            error: None,
        }
    }

    fn fetched(spec: &CompiledSpec, baseline: &ExpectedFixture, page: u8, body: &[u8]) -> Fetched {
        Fetched {
            url: spec.render_url(&request_for(baseline, page)).unwrap(),
            status: 200,
            body: body.to_vec(),
        }
    }

    #[test]
    fn removed_selector_fails_the_canary() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        // The drifted upstream no longer emits `div.result` containers.
        let drifted = b"<html><body><main>nope</main></body></html>".to_vec();
        let report = run(&spec, &baseline, &|req| {
            Ok(fetched(&spec, &baseline, req.page, &drifted))
        });
        assert!(!report.ok(), "removed selector must fail: {report:?}");
        assert!(report.failures.iter().any(|f| f.contains("0 results")));
    }

    #[test]
    fn page1_count_below_half_the_fixture_fails() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        let thin = html(
            &rows(4, "f")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        let report = run(&spec, &baseline, &|req| {
            Ok(fetched(&spec, &baseline, req.page, &thin))
        });
        assert!(!report.ok());
        assert!(report.failures.iter().any(|f| f.contains("under 50%")));
    }

    #[test]
    fn page2_serving_page1_again_fails_on_overlap() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        let body = html(
            &rows(10, "f")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        // Same body for both pages -> 100% url overlap (anti-bot pattern).
        let report = run(&spec, &baseline, &|req| {
            Ok(fetched(&spec, &baseline, req.page, &body))
        });
        assert!(!report.ok());
        assert!(report.failures.iter().any(|f| f.contains("page-1 urls")));
    }

    #[test]
    fn empty_fields_fail_the_fill_rate_check() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        // Live results keep the containers and urls but lose title/snippet
        // (empty link text: a missed field selector falls back to the
        // container's own text, which is empty here).
        let body = (0..10).fold(String::from("<html><body>"), |mut b, i| {
            b.push_str(&format!(
                r#"<div class="result"><a class="u" href="https://example.com/f{i}"></a></div>"#
            ));
            b
        }) + "</body></html>";
        let report = run(&spec, &baseline, &|req| {
            Ok(fetched(&spec, &baseline, req.page, body.as_bytes()))
        });
        assert!(!report.ok(), "{report:?}");
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.contains("title fill rate") && f.contains("0%")),
            "{report:?}"
        );
    }

    #[test]
    fn healthy_pages_pass() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        let p1 = html(
            &rows(10, "f")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        let p2 = html(
            &rows(10, "g")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        let report = run(&spec, &baseline, &|req| match req.page {
            1 => Ok(fetched(&spec, &baseline, 1, &p1)),
            _ => Ok(fetched(&spec, &baseline, 2, &p2)),
        });
        assert!(report.ok(), "{report:?}");
        assert_eq!(report.page1_count, 10);
        assert_eq!(report.page2_count, Some(10));
    }

    #[test]
    fn non_paginating_spec_skips_page2() {
        let spec = nopage_spec();
        let baseline = baseline_fixture(10);
        let calls = AtomicUsize::new(0);
        let body = html(
            &rows(10, "f")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        let report = run(&spec, &baseline, &|req| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(fetched(&spec, &baseline, req.page, &body))
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no page-2 fetch");
        assert_eq!(report.page2_count, None);
        assert!(report.ok());
    }

    #[test]
    fn page2_failure_is_a_note_not_a_failure() {
        let spec = spec();
        let baseline = baseline_fixture(10);
        let p1 = html(
            &rows(10, "f")
                .iter()
                .map(|(t, u, s)| (t.as_str(), u.as_str(), s.as_str()))
                .collect::<Vec<_>>(),
        );
        let report = run(&spec, &baseline, &|req| match req.page {
            1 => Ok(fetched(&spec, &baseline, 1, &p1)),
            _ => Err(EngineError::RateLimited),
        });
        assert!(report.ok(), "{report:?}");
        assert!(report.notes.iter().any(|n| n.contains("page 2")));
    }

    #[test]
    fn baseline_picks_first_sorted_success_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fixture");
        std::fs::create_dir_all(&dir).unwrap();
        // Error fixture sorts first alphabetically; the success one wins.
        std::fs::write(dir.join("aaa-error.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join("aaa-error.expected.json"),
            r#"{"query":"x","error":"blocked"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("zzz-ok.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join("zzz-ok.expected.json"),
            r#"{"query":"y","results":[{"title":"t","url":"https://a/","snippet":"s"}]}"#,
        )
        .unwrap();
        let (name, expected) = baseline(tmp.path(), "fixture").unwrap().unwrap();
        assert_eq!(name, "zzz-ok");
        assert_eq!(expected.query, "y");
    }

    #[test]
    fn baseline_none_when_only_error_fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fixture");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("err.html"), "<html></html>").unwrap();
        std::fs::write(
            dir.join("err.expected.json"),
            r#"{"query":"x","error":"blocked"}"#,
        )
        .unwrap();
        assert!(baseline(tmp.path(), "fixture").unwrap().is_none());
    }

    #[test]
    fn url_overlap_scores_shared_normalized_urls() {
        let mk = |url: &str| SearchResult {
            url: Url::parse(url).unwrap(),
            title: "t".into(),
            snippet: "s".into(),
            engine: cauce_core::EngineId::from("x"),
            published: None,
            score: 1.0,
        };
        let p1 = vec![
            mk("https://a/1"),
            mk("https://a/2"),
            mk("https://a/3"),
            mk("https://a/4"),
        ];
        let p2 = vec![
            mk("https://a/1"),
            mk("https://a/2"),
            mk("https://a/3"),
            mk("https://b/9"),
        ];
        assert_eq!(url_overlap(&p1, &p2), 0.75);
    }
}
