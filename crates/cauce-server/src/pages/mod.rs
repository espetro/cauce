//! Residual server-rendered pages (FX-06).
//!
//! The SPA at `/app` is the only UI layer; two read-only documents have no
//! client-side twin and stay server-rendered here instead of going through
//! a template engine:
//!
//! * [`trace`] — `GET /trace/{id}`, the request-trace replay view the SPA's
//!   audit tab links to (`/api/traces/{id}` stays the JSON twin).
//! * [`answer_view`] — `GET /answer/{id}`, the durable render of a stored
//!   `answer_log` row (`/api/answer-log/{id}` is the JSON twin).
//!
//! Both share the small inline stylesheet in [`shell`] — the design tokens
//! match the SPA's light/dark palette so the hand-off doesn't flash. The
//! SPA keeps a same-shaped `<noscript>` fallback in `index.html`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub mod answer_view;
pub mod trace;

use axum::response::Html;
use rust_i18n::t;

/// HTML-escape user/store data interpolated into a page body.
pub(crate) fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

/// `request_id`'s first UUID segment, the display form both pages use for
/// the footer chip. Full value stays on the chip's `title`.
pub(crate) fn short_id(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

/// The minimal inline stylesheet for the residual pages — same tokens the
/// SPA's shell sets in `index.html` (`--bg`/`--fg`/`--muted`/`--line`), so
/// system dark mode flips both the same way. Keep it small: these pages are
/// fallbacks, not a second UI.
const DOC_CSS: &str = concat!(
    ":root{--bg:#fff;--fg:#18181b;--muted:#71717a;--line:#e4e4e7;--accent:#0b6bcb;--warn:#b42318}",
    "@media(prefers-color-scheme:dark){:root{--bg:#0f0f11;--fg:#e4e4e7;--muted:#a1a1aa;",
    "--line:#27272a;--accent:#6aa8ff;--warn:#f66}}",
    "body{margin:0 auto;max-width:45rem;padding:1rem;font:15px/1.55 system-ui,sans-serif;",
    "background:var(--bg);color:var(--fg)}",
    "a{color:var(--accent)}",
    "header.site{display:flex;justify-content:space-between;align-items:baseline;gap:1rem;",
    "border-bottom:1px solid var(--line);padding:.4rem 0 .8rem;margin-bottom:1.2rem}",
    "header.site .brand{font-weight:700;color:var(--fg);text-decoration:none}",
    "header.site nav a{color:var(--muted);text-decoration:none}",
    "h1{font-size:1.15rem;margin:0 0 .6rem}",
    ".meta{color:var(--muted);font-size:.88em;display:flex;gap:.55rem;flex-wrap:wrap;",
    "align-items:baseline}",
    ".meta-chip{border:1px solid var(--line);border-radius:1em;padding:.04em .6em}",
    ".meta-chip.warn{border-color:var(--warn);color:var(--warn)}",
    ".request-id{margin-left:auto;font-family:ui-monospace,monospace}",
    "pre.trace{border:1px solid var(--line);border-radius:6px;padding:.7rem;overflow-x:auto;",
    "font-size:.85em}",
    "details.span summary{cursor:pointer}",
    "details.span pre{margin:.3rem 0 .9rem}",
    ".span-status-ok{color:#15803d}.span-status-error{color:var(--warn)}",
    ".turn-q{font-weight:600;font-size:1.05rem}",
    ".ungrounded,.field-error{color:var(--warn)}",
    ".section-label{color:var(--muted);text-transform:uppercase;font-size:.72em;",
    "letter-spacing:.06em}",
    ".source-card{border:1px solid var(--line);border-radius:6px;padding:.55rem .8rem;",
    "margin:.5rem 0}",
    ".source-card .host{color:var(--muted);font-size:.85em;margin-left:.4rem}",
    ".source-card .snippet{color:var(--muted);font-size:.9em;margin:.35rem 0 0}",
    ".cite-badge{color:var(--muted)}",
    ".related-question{margin:.2rem 0}",
    ".vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}",
);

/// The shared document frame: `<head>` shell (charset/viewport, opensearch
/// and favicon links, inline CSS, the pre-paint theme script so a saved
/// theme doesn't flash) plus the minimal `header.site` chrome. `body`
/// arrives already escaped where it carries user data.
pub(crate) fn doc(
    title: &str,
    heading: &str,
    back_href: &str,
    back_label: &str,
    body: &str,
) -> Html<String> {
    let brand = t!("common.brand");
    Html(format!(
        concat!(
            "<!doctype html><html lang=\"en\"><head>",
            "<meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
            "<title>{title} · {brand}</title>",
            "<link rel=\"search\" type=\"application/opensearchdescription+xml\" ",
            "title=\"{brand}\" href=\"/opensearch.xml\">",
            "<link rel=\"icon\" href=\"/favicon.ico\" type=\"image/svg+xml\">",
            "<style>{css}</style>",
            "<script>(()=>{{try{{var t=localStorage.getItem('cauce-theme');",
            "if(t==='light'||t==='dark')document.documentElement.dataset.theme=t}}catch(_){{}}}})()</script>",
            "</head><body>",
            "<header class=\"site\"><a class=\"brand\" href=\"/app/\">{brand}</a>",
            "<nav><a href=\"{back_href}\">{back_label}</a></nav></header>",
            "<main><h1>{heading}</h1>{body}</main>",
            "</body></html>",
        ),
        title = esc(title),
        brand = esc(&brand),
        css = DOC_CSS,
        back_href = back_href,
        back_label = esc(back_label),
        heading = esc(heading),
        body = body,
    ))
}
