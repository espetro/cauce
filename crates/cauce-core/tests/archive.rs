//! W5-01 acceptance: readability extraction against the 10-page fixture
//! corpus (markdown within 5 % of the recorded snapshot and containing
//! the sampled phrases) plus `fetch_and_index` end-to-end over a mock
//! HTTP origin — `pages` write, normalized-URL keying, redirect
//! resolution, the 1 MB fetch cap on a 3 MB page (capped + indexed
//! truncated), and the error mappings.
//!
//! `archive` is a non-default `cauce-core` feature (the minimal `mcp`
//! build must not pull the extraction deps), so the whole file gates on
//! it; `cargo test --workspace` still runs it — `cauce-server`'s default
//! `archive` feature unifies it into the workspace build.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.
#![cfg(feature = "archive")]

use std::sync::Arc;

use cauce_core::{
    ArchiveConfig, ArchiveError, Archiver, CacheKey, EngineError, MAX_FETCH_BYTES,
    MAX_MARKDOWN_BYTES, Store,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(dead_code)]
mod support;
use support::StubStore;

/// Base URL the `.expected.md` snapshots were generated with (relative
/// links resolve against it).
const BASE: &str = "https://example.com/";

/// (fixture stem, phrases the extracted markdown must contain).
const FIXTURES: &[(&str, &[&str])] = &[
    (
        "01-news-article",
        &[
            "Harbor & Trade",
            "night-time dredging",
            "Tidewater Marine",
            "work slowdown",
        ],
    ),
    (
        "02-blog-post",
        &[
            "September 28, 2025",
            "hand-written SQL",
            "ORMs are excellent",
        ],
    ),
    (
        "03-docs-page",
        &[
            "Rate limiting",
            "token bucket",
            "429 Too Many Requests",
            "X-RateLimit-Limit",
        ],
    ),
    (
        "04-recipe",
        &["Twenty-five minutes", "salmon fillets", "white miso paste"],
    ),
    (
        "05-forum-thread",
        &["uncorrectable_errors", "wren42", "iodine", "smartctl"],
    ),
    ("06-product", &["$129.00", "Atlas Pro", "412 reviews"]),
    (
        "07-essay",
        &["mechanical watch", "Quartz won", "Patek Philippe"],
    ),
    (
        "08-listicle",
        &["pressure cooker", "Instant Pot", "eleven weeks"],
    ),
    (
        "09-changelog",
        &["Streaming compaction", "VACUUM ANALYZE", "PARALLEL"],
    ),
    ("10-minimal", &["grid, drill, tag", "patience as a method"]),
];

fn fixture_html(name: &str) -> &'static str {
    match name {
        "01-news-article" => include_str!("fixtures/archive/01-news-article.html"),
        "02-blog-post" => include_str!("fixtures/archive/02-blog-post.html"),
        "03-docs-page" => include_str!("fixtures/archive/03-docs-page.html"),
        "04-recipe" => include_str!("fixtures/archive/04-recipe.html"),
        "05-forum-thread" => include_str!("fixtures/archive/05-forum-thread.html"),
        "06-product" => include_str!("fixtures/archive/06-product.html"),
        "07-essay" => include_str!("fixtures/archive/07-essay.html"),
        "08-listicle" => include_str!("fixtures/archive/08-listicle.html"),
        "09-changelog" => include_str!("fixtures/archive/09-changelog.html"),
        "10-minimal" => include_str!("fixtures/archive/10-minimal.html"),
        _ => panic!("unknown fixture {name}"),
    }
}

fn fixture_expected(name: &str) -> &'static str {
    match name {
        "01-news-article" => include_str!("fixtures/archive/01-news-article.expected.md"),
        "02-blog-post" => include_str!("fixtures/archive/02-blog-post.expected.md"),
        "03-docs-page" => include_str!("fixtures/archive/03-docs-page.expected.md"),
        "04-recipe" => include_str!("fixtures/archive/04-recipe.expected.md"),
        "05-forum-thread" => include_str!("fixtures/archive/05-forum-thread.expected.md"),
        "06-product" => include_str!("fixtures/archive/06-product.expected.md"),
        "07-essay" => include_str!("fixtures/archive/07-essay.expected.md"),
        "08-listicle" => include_str!("fixtures/archive/08-listicle.expected.md"),
        "09-changelog" => include_str!("fixtures/archive/09-changelog.expected.md"),
        "10-minimal" => include_str!("fixtures/archive/10-minimal.expected.md"),
        _ => panic!("unknown fixture {name}"),
    }
}

/// Loopback-permitting archiver for the wiremock tests (#189): the
/// mock origin is always 127.0.0.1, which the default-on egress guard
/// refuses — `archive.allow_private` is the documented opt-in the guard
/// tests below leave off.
fn archiver(store: &Arc<StubStore>) -> Archiver {
    Archiver::new(
        store.clone(),
        &ArchiveConfig {
            allow_private: true,
            ..ArchiveConfig::default()
        },
    )
    .expect("archiver builds")
}

/// Guarded archiver (`allow_private` off, the shipped default).
fn guarded_archiver(store: &Arc<StubStore>) -> Archiver {
    Archiver::new(store.clone(), &ArchiveConfig::default()).expect("archiver builds")
}

/// Acceptance: every fixture extracts to markdown within 5 % of the
/// snapshot length and containing the sampled phrases.
#[test]
fn fixtures_match_snapshot_and_phrases() {
    for (name, phrases) in FIXTURES {
        let extracted = cauce_core::archive::extract(fixture_html(name), BASE)
            .unwrap_or_else(|e| panic!("{name}: extraction failed: {e}"));
        let expected = fixture_expected(name);
        let diff =
            extracted.markdown.len().abs_diff(expected.len()) as f64 / expected.len().max(1) as f64;
        assert!(
            diff <= 0.05,
            "{name}: markdown len {} vs snapshot {} ({:.1}% off)",
            extracted.markdown.len(),
            expected.len(),
            diff * 100.0,
        );
        for phrase in *phrases {
            assert!(
                extracted.markdown.contains(phrase),
                "{name}: markdown missing sampled phrase {phrase:?}",
            );
        }
        assert!(
            !extracted.title.is_empty(),
            "{name}: readability resolved no title",
        );
    }
}

/// The exit criterion: one `fetch_and_index` call yields a `pages` row —
/// keyed by the normalized final URL (tracking params stripped before
/// fetch), `source_query_hash` carried through.
#[tokio::test]
async fn fetch_and_index_writes_normalized_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture_html("01-news-article")))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let archiver = archiver(&store);
    let key = CacheKey::from(&support::req("archive write"));
    let row = archiver
        .fetch_and_index(
            &format!("{}/article?utm_source=newsletter", server.uri()),
            Some(key.clone()),
        )
        .await
        .expect("fetch_and_index");

    assert_eq!(row.url.as_str(), format!("{}/article", server.uri()));
    assert!(!row.title.is_empty());
    assert!(row.markdown.contains("night-time dredging"));
    assert_eq!(row.byte_len as usize, fixture_html("01-news-article").len());
    assert_eq!(row.source_query_hash.as_ref(), Some(&key));

    // And it reads back through the store the same way
    // `GET /api/pages/{url}` will.
    let got = store
        .get_page(&row.url)
        .await
        .expect("get_page")
        .expect("row indexed");
    assert_eq!(got.markdown, row.markdown);
}

/// Redirects resolve to the final URL: the row is keyed where the content
/// actually came from.
#[tokio::test]
async fn redirect_indexes_under_final_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/old"))
        .respond_with(ResponseTemplate::new(301).append_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture_html("10-minimal")))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let row = archiver(&store)
        .fetch_and_index(&format!("{}/old", server.uri()), None)
        .await
        .expect("fetch_and_index");
    assert_eq!(row.url.as_str(), format!("{}/new", server.uri()));
}

/// Acceptance: a page bigger than the 1 MB fetch cap is capped (body cut
/// at [`MAX_FETCH_BYTES`], `warn!`ed on the `archive_fetch` span) and the
/// truncated content still indexes — stored markdown never exceeds
/// [`MAX_MARKDOWN_BYTES`].
#[tokio::test]
async fn oversized_page_is_capped_and_indexed() {
    // ~3 MB of extractable article: a real headline plus repeated
    // paragraphs, so even the 1 MB cut yields plenty of readable content.
    let paragraph = "<p>The survey team worked the grid methodically, \
                     logging every stake and flag as the flats baked under \
                     a midday sun that pushed surface temperature past \
                     fifty degrees.</p>\n";
    let mut body = String::from(
        "<html><head><title>Oversized report</title></head><body><article>\
         <h1>Field report</h1>",
    );
    while body.len() < 3 * MAX_FETCH_BYTES {
        body.push_str(paragraph);
    }
    body.push_str("</article></body></html>");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/huge"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let row = archiver(&store)
        .fetch_and_index(&format!("{}/huge", server.uri()), None)
        .await
        .expect("truncated page still indexes");

    assert_eq!(row.byte_len as usize, MAX_FETCH_BYTES);
    assert!(row.markdown.len() <= MAX_MARKDOWN_BYTES);
    assert!(row.markdown.contains("survey team"));
}

/// Non-2xx upstreams surface `ArchiveError::Status` and write nothing.
#[tokio::test]
async fn upstream_error_writes_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let err = archiver(&store)
        .fetch_and_index(&format!("{}/gone", server.uri()), None)
        .await
        .expect_err("404 maps to Status");
    assert!(matches!(err, ArchiveError::Status(404)));
    assert!(store.pages.lock().unwrap().is_empty());
}

/// Rejected inputs are `ArchiveError::InvalidUrl` before any network.
#[tokio::test]
async fn invalid_urls_are_rejected() {
    let store = Arc::new(StubStore::default());
    let archiver = archiver(&store);
    for bad in ["not a url"] {
        let err = archiver
            .fetch_and_index(bad, None)
            .await
            .expect_err("rejected");
        assert!(matches!(err, ArchiveError::InvalidUrl(_)), "{bad}: {err}");
    }
    // Parseable but not http(s): refused by the scheme allowlist, not as
    // a malformed URL — same `Blocked` class as a private address.
    for bad in [
        "ftp://example.com/f",
        "javascript:alert(1)",
        "file:///etc/passwd",
    ] {
        let err = archiver
            .fetch_and_index(bad, None)
            .await
            .expect_err("rejected");
        assert!(matches!(err, ArchiveError::Blocked(_)), "{bad}: {err}");
    }
}

/// A page readability cannot score is `ArchiveError::Extract`, not a
/// silently empty row.
#[tokio::test]
async fn empty_extraction_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/blank"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<html><head><title>t</title></head><body></body></html>"),
        )
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let err = archiver(&store)
        .fetch_and_index(&format!("{}/blank", server.uri()), None)
        .await
        .expect_err("no readable content");
    assert!(matches!(err, ArchiveError::Extract(_)));
}

/// Zero politeness knobs are a build error, not a panic (mirrors
/// `HttpClient`'s contract; surfaced as `ArchiveError::Fetch`).
#[test]
fn zero_bucket_knobs_fail_to_build() {
    let store = Arc::new(StubStore::default());
    let cfg = ArchiveConfig {
        index_on_click: true,
        requests_per_second: 0,
        burst: 2,
        allow_private: true,
    };
    let err = Archiver::new(store, &cfg).expect_err("rps=0 must fail");
    assert!(matches!(
        err,
        ArchiveError::Fetch(EngineError::Transport(_))
    ));
}

/// #189: the egress guard refuses private/reserved targets — literal IPs
/// and `localhost` alike — before any connect, surfacing
/// `ArchiveError::Blocked` (403 on `POST /api/pages`, `invalid_params`
/// on MCP `fetch_and_index`), and writes nothing.
#[tokio::test]
async fn private_targets_are_blocked() {
    // Nothing mounted: a guard-bypassing fetcher would still be observed
    // by `received_requests` below.
    let server = MockServer::start().await;
    let uri = server.uri();

    let store = Arc::new(StubStore::default());
    let archiver = guarded_archiver(&store);
    for target in [
        "http://127.0.0.1/",
        "http://169.254.169.254/latest/meta-data",
        "http://10.0.0.1/",
        "http://[::1]/",
        "http://[fd00::1]/",
        uri.as_str(),
        // `localhost` is a real lookup resolving to loopback.
        "http://localhost:1/",
    ] {
        let err = archiver
            .fetch_and_index(target, None)
            .await
            .expect_err("private target must be blocked");
        assert!(matches!(err, ArchiveError::Blocked(_)), "{target}: {err}");
    }
    assert!(
        server
            .received_requests()
            .await
            .expect("requests readable")
            .is_empty(),
        "egress guard allowed a connect"
    );
    assert!(store.pages.lock().unwrap().is_empty());
}

/// Redirect hops get the same treatment as hop 0: a 302 pointing at a
/// non-http(s) scheme fails the fetch — it must never be followed.
/// (Per-hop *address* validation can't be exercised against wiremock:
/// with the guard on, a loopback origin is rejected at hop 0 already;
/// the `archive::fetch` resolver unit tests cover the per-hop path.)
#[tokio::test]
async fn redirect_to_non_http_scheme_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/old"))
        .respond_with(ResponseTemplate::new(302).append_header("location", "file:///etc/passwd"))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let err = archiver(&store)
        .fetch_and_index(&format!("{}/old", server.uri()), None)
        .await
        .expect_err("file: redirect must fail");
    assert!(matches!(err, ArchiveError::Blocked(_)), "{err}");
}

/// `[archive] allow_private` is the opt-in: with it set, the same
/// loopback targets the guard blocks are fetched normally (every other
/// test in this file runs through it).
#[tokio::test]
async fn allow_private_reaches_loopback() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture_html("10-minimal")))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    archiver(&store)
        .fetch_and_index(&format!("{}/page", server.uri()), None)
        .await
        .expect("guard must not reject when allow_private is set");
}
