//! W2-08 DOM shell assertions: the cheap stand-in for screenshot
//! baselines (the maintenance-review amendment swapped Playwright
//! baselines for this).
//!
//! FX-06 rewrote the contract: the Svelte SPA at `/app` is the only UI
//! layer, so this file asserts (a) the legacy canonical paths permanently
//! redirect onto their `/app` twins, (b) the served SPA shell carries the
//! head/body contract the app mounts into — viewport meta, the pre-paint
//! theme script, the `<link rel="search">` and `<noscript>` fallback — and
//! (c) the residual server-rendered pages (`/trace/{id}`, `/answer/{id}`)
//! keep the small document shell of `src/pages/`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The browser surface exists only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use axum::Router;
use axum::http::StatusCode;

mod support;
use support::*;

/// Upper bound on rendered elements after script/style bodies are
/// stripped; the residual pages render well under a hundred.
const MAX_ELEMENTS: usize = 2000;

/// Drop `<script>...</script>` (and `<style>...</style>`) bodies so the
/// element/landmark counts measure the DOM, not inlined sources. Cheap
/// linear scan — the pages never nest a literal closing tag inside those
/// bodies.
fn strip_tag_bodies(html: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(&open) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(&close) {
            Some(end) => &rest[start + end + close.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// DOM-only view of a rendered page: script and style bodies removed.
fn dom(html: &str) -> String {
    strip_tag_bodies(&strip_tag_bodies(html, "script"), "style")
}

/// Count element open tags (`<` followed by an ASCII letter) — a cheap
/// DOM-size proxy good enough for a sanity bound.
fn element_count(html: &str) -> usize {
    html.as_bytes()
        .windows(2)
        .filter(|w| w[0] == b'<' && w[1].is_ascii_alphabetic())
        .count()
}

/// Landmark sanity shared by every served document: balanced html/body,
/// exactly one each, and a bounded element count.
fn assert_document(uri: &str, body: &str) {
    assert!(
        body.contains(r#"name="viewport" content="width=device-width, initial-scale=1""#),
        "{uri}: missing viewport meta"
    );
    // The pre-paint theme script — a saved light/dark choice must not
    // flash the opposite palette on any browser surface.
    assert!(
        body.contains("localStorage.getItem(\"cauce-theme\")")
            || body.contains("localStorage.getItem('cauce-theme')"),
        "{uri}: missing theme-init script"
    );
    let dom = dom(body);
    for tag in ["html", "body"] {
        let opens = count(&dom, &format!("<{tag}"));
        let closes = count(&dom, &format!("</{tag}>"));
        assert_eq!(opens, closes, "{uri}: unbalanced <{tag}>");
        assert_eq!(opens, 1, "{uri}: expected exactly one <{tag}>");
    }
    let elements = element_count(&dom);
    assert!(
        elements < MAX_ELEMENTS,
        "{uri}: {elements} elements exceeds the {MAX_ELEMENTS} sanity bound"
    );
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// `GET /app` (and every client route): the shell document plus the
/// mount point, module script and the styled `<noscript>` fallback the
/// §7.5 decision keeps.
fn assert_spa_shell(uri: &str, body: &str) {
    assert_document(uri, body);
    assert!(
        body.contains(r#"<div id="app"></div>"#),
        "{uri}: missing mount point"
    );
    assert!(
        body.contains(r#"<script type="module""#),
        "{uri}: missing module script tag"
    );
    assert!(
        body.contains(r#"<link rel="search" type="application/opensearchdescription+xml""#)
            && body.contains(r#"href="/opensearch.xml""#),
        "{uri}: missing opensearch link"
    );
    assert!(
        body.contains("<noscript>"),
        "{uri}: missing noscript fallback"
    );
    assert!(
        body.contains("prefers-color-scheme") && body.contains("--bg:"),
        "{uri}: noscript shell lost the theme tokens"
    );
}

/// The residual server-rendered pages' shell: minimal `header.site`
/// chrome, one `<main>`, the shared inline stylesheet's light/dark token
/// blocks, and balanced landmarks.
fn assert_doc_shell(uri: &str, body: &str) {
    assert_document(uri, body);
    assert!(
        body.contains(r#"<header class="site">"#),
        "{uri}: missing site header"
    );
    let dom = dom(body);
    assert_eq!(
        count(&dom, "<main"),
        count(&dom, "</main>"),
        "{uri}: unbalanced <main>"
    );
    assert_eq!(
        count(&dom, "<main"),
        1,
        "{uri}: expected exactly one <main>"
    );
    assert_eq!(
        count(&dom, r#"<header class="site">"#),
        1,
        "{uri}: expected exactly one site header"
    );
    assert!(
        body.contains("prefers-color-scheme") && body.contains("--bg:"),
        "{uri}: doc shell lost the theme tokens"
    );
    assert!(
        body.contains(r#"class="request-id""#),
        "{uri}: missing request-id footer chip"
    );
}

/// 308 onto the `/app` twin, query string preserved.
async fn assert_redirect(router: &Router, uri: &str, expected: &str) {
    let (status, headers, body) = get_headers(router, uri).await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT, "{uri}: {body}");
    assert_eq!(
        headers["location"].to_str().unwrap(),
        expected,
        "{uri}: wrong redirect target"
    );
}

/// The legacy canonical paths FX-01..05 moved under `/app` redirect
/// permanently (FX-06); browser bookmarks and the opensearch results URL
/// keep landing in the SPA.
#[tokio::test]
async fn legacy_paths_redirect_to_app() {
    let (router, _state, _tmp) = app();
    for (uri, target) in [
        ("/", "/app/"),
        ("/search", "/app/search"),
        ("/search?q=shell&page=2", "/app/search?q=shell&page=2"),
        ("/answer", "/app/answer"),
        ("/answer?q=x", "/app/answer?q=x"),
    ] {
        assert_redirect(&router, uri, target).await;
    }
}

/// Every mounted `/app` route serves the same shell (embedded asset hits
/// stay out of scope — `spa_nested` answers them before the fallback).
#[tokio::test]
async fn app_routes_serve_the_spa_shell() {
    let (router, _state, _tmp) = app();
    for uri in ["/app", "/app/search", "/app/admin?tab=engines"] {
        let (status, body) = get_html(&router, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_spa_shell(uri, &body);
    }
}

/// The residual server-rendered pages keep the doc shell: `/trace/{id}`
/// renders the frame even when the JSONL has no records yet, and
/// `/answer/{id}` renders a seeded `answer_log` row.
#[tokio::test]
async fn residual_pages_render_the_doc_shell() {
    let (router, state, _tmp) = app();

    // Seed a search so a real request id exists for /trace/<id>.
    let (status, seeded) = get_json(&router, "/api/search?q=shell").await;
    assert_eq!(status, StatusCode::OK, "{seeded}");
    let rid = seeded["meta"]["request_id"]
        .as_str()
        .expect("meta.request_id")
        .to_string();

    let (status, body) = get_html(&router, &format!("/trace/{rid}")).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "/trace answered {status}"
    );
    assert_doc_shell("/trace/{id}", &body);
    assert!(body.contains(&rid), "/trace: traced id missing");

    // A seeded answer_log row renders the durable read-only page.
    let log_id = state
        .store()
        .log_answer(cauce_core::AnswerLogRow {
            id: None,
            ts: chrono::Utc::now(),
            query: "what is rust".to_string(),
            query_raw: None,
            model: "test-model".to_string(),
            answer: "A systems language.".to_string(),
            confidence: Some(8),
            sources: vec![cauce_core::AnswerSource {
                url: url::Url::parse("https://example.com/rust").unwrap(),
                title: "Rust".to_string(),
                snippet: "…".to_string(),
                engine: cauce_core::EngineId::from("replay"),
            }],
            related_questions: vec!["why rust".to_string()],
            request_id: None,
            client: cauce_core::ClientKind::Ui,
            origin: cauce_core::SearchOrigin::User,
            status: cauce_core::AnswerStatus::Done,
            ungrounded: false,
            error: None,
        })
        .await
        .expect("seed answer_log");
    let (status, body) = get_html(&router, &format!("/answer/{log_id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_doc_shell("/answer/{id}", &body);
    assert!(body.contains("what is rust"), "{body}");
    assert!(body.contains(r#"class="source-card""#), "{body}");
}
