//! W2-08 DOM shell assertions: the cheap stand-in for screenshot
//! baselines (the maintenance-review amendment swapped Playwright
//! baselines for this). Every mounted HTML page renders on the `replay`
//! engine and must carry the same shell contract: the viewport meta, the
//! shared `header.site` nav (primary + operator groups, `settings`, the
//! theme toggle), both light and dark CSS variable blocks in the inlined
//! stylesheet, a sane element count, and balanced landmarks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use axum::http::StatusCode;

mod support;
use support::*;

/// Upper bound on rendered elements after script/style bodies are
/// stripped; the heaviest page today renders well under a thousand.
const MAX_ELEMENTS: usize = 2000;

/// Drop `<script>...</script>` (and `<style>...</style>`) bodies so the
/// element/landmark counts measure the DOM, not the inlined htmx, theme
/// or stylesheet sources. Cheap linear scan — the templates never nest
/// a literal closing tag inside those bodies.
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

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The shell contract every page shares, asserted once per render.
fn assert_shell(uri: &str, body: &str, expect_current: Option<&str>) {
    assert!(
        body.contains(r#"<meta name="viewport" content="width=device-width, initial-scale=1">"#),
        "{uri}: missing viewport meta"
    );
    // Light + dark variable blocks in the emitted <style>: the light
    // `:root`, the dark media-query block and the toggle's
    // `[data-theme="dark"]` override.
    assert!(
        body.contains(":root") && body.contains("--bg:") && body.contains("--fg:"),
        "{uri}: missing light CSS variables"
    );
    assert!(
        body.contains("prefers-color-scheme: dark"),
        "{uri}: missing dark media-query block"
    );
    assert!(
        body.contains(r#":root[data-theme="dark"]"#),
        "{uri}: missing dark data-theme variable block"
    );
    // The no-flash head script and the real toggle (never a hidden
    // fixture: dark arrives only via data-theme/localStorage).
    assert!(
        body.contains("localStorage.getItem(\"cauce-theme\")"),
        "{uri}: missing theme-init script"
    );
    assert!(
        body.contains("id=\"theme-toggle\""),
        "{uri}: missing theme toggle"
    );
    // The shared header: brand, both nav tiers, settings, the `more`
    // menu and its <700px collapse rules.
    assert!(
        body.contains("<header class=\"site\">"),
        "{uri}: missing shared header"
    );
    assert!(
        body.contains("nav-primary") && body.contains("nav-operator"),
        "{uri}: missing nav tiers"
    );
    assert!(
        body.contains("details class=\"nav-more\"") || body.contains("class=\"nav-more\""),
        "{uri}: missing collapsed more menu"
    );
    assert!(
        body.contains("width < 700px"),
        "{uri}: missing <700px header collapse rule"
    );
    assert!(
        body.contains("href=\"/settings\""),
        "{uri}: missing settings link"
    );
    // Landmark sanity on the script-stripped DOM: balanced
    // html/body/main, exactly one each; header elements may repeat
    // (engine cards carry `<header class="engine-head">`) but the page
    // shell contributes exactly one `<header class="site">`.
    let dom = dom(body);
    for tag in ["html", "body", "main"] {
        let opens = count(&dom, &format!("<{tag}"));
        let closes = count(&dom, &format!("</{tag}>"));
        assert_eq!(opens, closes, "{uri}: unbalanced <{tag}>");
        assert_eq!(opens, 1, "{uri}: expected exactly one <{tag}>");
    }
    assert_eq!(
        count(&dom, "<header"),
        count(&dom, "</header>"),
        "{uri}: unbalanced <header>"
    );
    assert_eq!(
        count(&dom, "<header class=\"site\">"),
        1,
        "{uri}: expected exactly one shared site header"
    );
    match expect_current {
        Some(href) => assert!(
            body.contains(&format!("href=\"{href}\" aria-current=\"page\"")),
            "{uri}: expected aria-current on {href}:\n{}",
            &body[..body.len().min(4000)]
        ),
        None => assert!(
            !dom.contains("aria-current"),
            "{uri}: no nav link should be current"
        ),
    }
    let elements = element_count(&dom);
    assert!(
        elements < MAX_ELEMENTS,
        "{uri}: {elements} elements exceeds the {MAX_ELEMENTS} sanity bound"
    );
}

/// Table-driven over every mounted page route (tests/routes.rs owns the
/// mount table; this asserts the rendered shell of each row).
#[tokio::test]
async fn every_page_renders_the_shared_shell() {
    let (router, _state, _tmp) = app();

    // Seed a search so history/dashboard/cache render populated states
    // and a real request id exists for /trace/<id>.
    let (status, seeded) = get_json(&router, "/api/search?q=shell").await;
    assert_eq!(status, StatusCode::OK, "{seeded}");
    let rid = seeded["meta"]["request_id"]
        .as_str()
        .expect("meta.request_id")
        .to_string();

    let pages: &[(&str, Option<&str>)] = &[
        ("/", Some("/")),
        ("/search?q=shell", Some("/")),
        ("/search?q=shell&stream=1", Some("/")),
        ("/history", Some("/history")),
        ("/dashboard", Some("/dashboard")),
        ("/cache", Some("/cache")),
        ("/engines", Some("/engines")),
        ("/audit", Some("/audit")),
        ("/settings", Some("/settings")),
    ];
    for (uri, current) in pages {
        let (status, body) = get_html(&router, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_shell(uri, &body, *current);
    }

    // /trace/<id> of an issued request id renders the page frame even
    // when the JSONL has no records yet (404 body keeps the shell).
    let (status, body) = get_html(&router, &format!("/trace/{rid}")).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "/trace answered {status}"
    );
    assert_shell("/trace/{id}", &body, None);
}

/// The header's `more` menu and the operator group name the same three
/// destinations — a drift guard for the two renderings of one tier.
#[tokio::test]
async fn operator_group_matches_more_menu() {
    let (router, _state, _tmp) = app();
    let (status, body) = get_html(&router, "/").await;
    assert_eq!(status, StatusCode::OK);
    for href in ["/engines", "/cache", "/audit"] {
        assert_eq!(
            count(&body, &format!("href=\"{href}\"")),
            2,
            "{href} should appear once in .nav-operator and once in .nav-more"
        );
    }
    // The operator group is CSS-quieted, and it collapses below 700 px
    // where `more` takes over.
    assert!(body.contains("nav.nav-operator"));
    assert!(body.contains("details.nav-more"));
}
