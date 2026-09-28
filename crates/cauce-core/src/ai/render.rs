//! Server-side render of answer markdown to sanitized HTML (issue #226):
//! the `done` frame carries the markup beside the raw `answer` text so
//! any client — the TS bundle today, the Svelte SPA (#223) or an FX
//! client — can inject it directly.
//!
//! Rendering runs a pulldown-cmark event pass that rewrites two node
//! kinds before ammonia sees the output: `[n]` citation markers in
//! plain text become `<a class="cite" data-cite="n" href="#cite-n">`
//! placeholders (the client retargets the fragment to its turn-scoped
//! `src-<turn>-<n>` / `asrc-<n>` cards), and `[text](url)` links are
//! emitted with `target="_blank" rel="noopener noreferrer"` (the
//! `history.html` convention). Text inside code blocks, link titles and
//! image alt text is never citation-scanned.
//!
//! Sanitization is a tight allowlist — prose, code, quotes, table
//! basics — plus `data-cite`/`cite` only on anchors. Script/style/
//! iframe/form content is dropped whole, not kept as text.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html::push_html};
use pulldown_cmark_escape::{escape_href, escape_html};

/// Markdown extensions for answer bodies: tables and strikethrough are
/// the constructs models actually emit; everything else stays off.
const MD_OPTIONS: Options = Options::ENABLE_TABLES.union(Options::ENABLE_STRIKETHROUGH);

/// The sanitizer, built once. Allowlist per issue #226: `h1..h6`, `p`,
/// `a`, lists, `code`/`pre`, `blockquote`, `strong`/`em`/`del`, `br`/`hr`
/// and table basics. Attributes: `a` gets `href`/`title`/`target`/`rel`/
/// `data-cite` with `class` restricted to `cite` (allowed_classes, not a
/// free-form class attribute); `code` keeps `class` for `language-*`;
/// `th`/`td` keep pulldown's `align`. URLs: http/https/mailto only —
/// `javascript:`/`data:` hrefs lose the attribute. Relative hrefs pass
/// through so the `#cite-N` placeholders survive; ammonia's default
/// `link_rel` (`noopener noreferrer`) then applies to every link that
/// kept an `href`, placeholder anchors included.
fn sanitizer() -> &'static Builder<'static> {
    static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
        let mut b = Builder::new();
        b.tags(HashSet::from([
            "a",
            "blockquote",
            "br",
            "code",
            "del",
            "em",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "hr",
            "li",
            "ol",
            "p",
            "pre",
            "strong",
            "table",
            "tbody",
            "td",
            "th",
            "thead",
            "tr",
            "ul",
        ]));
        // `rel` is deliberately absent: ammonia's link_rel owns it
        // (listing it here too is a hard panic).
        b.tag_attributes(HashMap::from([
            ("a", HashSet::from(["href", "title", "target", "data-cite"])),
            ("code", HashSet::from(["class"])),
            ("th", HashSet::from(["align"])),
            ("td", HashSet::from(["align"])),
        ]));
        b.allowed_classes(HashMap::from([("a", HashSet::from(["cite"]))]));
        b.url_schemes(HashSet::from(["http", "https", "mailto"]));
        b.url_relative(UrlRelative::PassThrough);
        b.clean_content_tags(HashSet::from([
            "script", "style", "iframe", "form", "object", "embed",
        ]));
        b
    });
    &SANITIZER
}

/// Render `markdown` to sanitized HTML for the `done` frame's `html`
/// field. `[n]` markers with `1 <= n <= n_sources` become citation
/// anchors; out-of-range markers and markers inside code stay literal
/// text (the model cited a source it was never shown).
pub fn render_answer_html(markdown: &str, n_sources: usize) -> String {
    let events = linkify(Parser::new_ext(markdown, MD_OPTIONS), n_sources);
    let mut raw = String::with_capacity(markdown.len() + markdown.len() / 2);
    push_html(&mut raw, events.into_iter());
    sanitizer().clean(&raw).to_string()
}

/// The open tag of a rewritten markdown link: escaped destination,
/// the `target`/`rel` convention, and the title when present.
fn link_open(dest_url: &str, title: &str) -> String {
    let mut s = String::with_capacity(dest_url.len() + title.len() + 48);
    s.push_str("<a href=\"");
    escape_href(&mut s, dest_url).expect("String write is infallible");
    s.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\"");
    if !title.is_empty() {
        s.push_str(" title=\"");
        escape_html(&mut s, title).expect("String write is infallible");
        s.push('"');
    }
    s.push('>');
    s
}

/// Emit `text` split on `[n]` markers: in-range markers become
/// `Event::InlineHtml` cite anchors, everything else `Event::Text`.
/// Called on tag boundaries and at the end of the walk so a marker
/// split across chunked `Text` events still links.
fn flush_text<'a>(out: &mut Vec<Event<'a>>, text: &mut String, n_sources: usize) {
    if text.is_empty() {
        return;
    }
    let mut last = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let digits = &text[i + 1..];
        let len = digits.bytes().take_while(|b| b.is_ascii_digit()).count();
        if len == 0 || digits.as_bytes().get(len) != Some(&b']') {
            i += 1;
            continue;
        }
        let n: usize = digits[..len].parse().unwrap_or(usize::MAX);
        if n < 1 || n > n_sources {
            i += 1 + len + 1;
            continue;
        }
        if i > last {
            out.push(Event::Text(CowStr::from(text[last..i].to_string())));
        }
        out.push(Event::InlineHtml(CowStr::from(format!(
            "<a class=\"cite\" data-cite=\"{n}\" href=\"#cite-{n}\">[{n}]</a>"
        ))));
        i += 1 + len + 1;
        last = i;
    }
    if text.len() > last {
        out.push(Event::Text(CowStr::from(text[last..].to_string())));
    }
    text.clear();
}

/// Walk the parser stream once: `Text` outside code/link/image contexts
/// accumulates for citation scanning; markdown links are re-emitted as
/// inline HTML with the target/rel attributes ammonia can't add itself;
/// everything else passes through.
fn linkify<'a>(parser: Parser<'a>, n_sources: usize) -> Vec<Event<'a>> {
    let mut out = Vec::new();
    // Plain text awaiting the [n] scan. Code, link text and image alt
    // text bypass the buffer so citations never land inside them.
    let mut text = String::new();
    let mut in_code = false;
    let mut in_link = false;
    let mut in_image = false;
    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                flush_text(&mut out, &mut text, n_sources);
                in_code = true;
                out.push(event);
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_text(&mut out, &mut text, n_sources);
                in_code = false;
                out.push(event);
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                flush_text(&mut out, &mut text, n_sources);
                out.push(Event::InlineHtml(CowStr::from(link_open(
                    &dest_url, &title,
                ))));
                in_link = true;
            }
            Event::End(TagEnd::Link) => {
                flush_text(&mut out, &mut text, n_sources);
                out.push(Event::InlineHtml(CowStr::from("</a>")));
                in_link = false;
            }
            Event::Start(Tag::Image { .. }) => {
                flush_text(&mut out, &mut text, n_sources);
                in_image = true;
                out.push(event);
            }
            Event::End(TagEnd::Image) => {
                flush_text(&mut out, &mut text, n_sources);
                in_image = false;
                out.push(event);
            }
            Event::Text(t) if !in_code && !in_link && !in_image => text.push_str(&t),
            _ => {
                flush_text(&mut out, &mut text, n_sources);
                out.push(event);
            }
        }
    }
    flush_text(&mut out, &mut text, n_sources);
    out
}

#[cfg(test)]
mod tests {
    use super::render_answer_html;

    fn render(md: &str, n_sources: usize) -> String {
        render_answer_html(md, n_sources)
    }

    #[test]
    fn prose_and_structure_render() {
        let html = render(
            "# Title\n\nSome **bold** and *em* with `code`.\n\n- a\n- b\n",
            0,
        );
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(html.contains("<em>em</em>"), "{html}");
        assert!(html.contains("<code>code</code>"), "{html}");
        assert!(
            html.contains("<ul>") && html.contains("<li>a</li>"),
            "{html}"
        );
    }

    #[test]
    fn quotes_code_and_tables_render() {
        let html = render(
            "> quoted\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
            0,
        );
        assert!(html.contains("<blockquote>"), "{html}");
        assert!(
            html.contains("<pre>") && html.contains("let x = 1;"),
            "{html}"
        );
        assert!(
            html.contains("<table>") && html.contains("<td>1</td>"),
            "{html}"
        );
    }

    #[test]
    fn citations_become_anchors_only_in_range() {
        let html = render("facts [1] and [2], not [3] or [0]", 2);
        assert!(
            html.contains("<a class=\"cite\" data-cite=\"1\" href=\"#cite-1\""),
            "{html}"
        );
        assert!(html.contains("data-cite=\"2\""), "{html}");
        assert!(html.contains("[3]") && !html.contains("cite-3"), "{html}");
        assert!(!html.contains("cite-0"), "{html}");
    }

    #[test]
    fn citations_split_across_text_events_still_link() {
        // A soft line break between "[1]" and later text is the common
        // multi-Text-event shape; the buffer must join them.
        let html = render("see [1]\nmore text", 1);
        assert!(html.contains("data-cite=\"1\""), "{html}");
    }

    #[test]
    fn citations_skip_code_and_link_text() {
        let html = render(
            "`[1]` inline\n\n```\n[2] block\n```\n\n[[1]](https://e.com)",
            3,
        );
        assert!(html.contains("<code>[1]</code>"), "{html}");
        assert!(html.contains("[2] block"), "{html}");
        // The link's inner text stays text — no nested anchors.
        assert_eq!(html.matches("data-cite").count(), 0, "{html}");
        assert!(html.contains("[1]</a>"), "{html}");
    }

    #[test]
    fn links_get_target_and_rel() {
        let html = render("[rust](https://rust-lang.org \"the rust site\")", 0);
        assert!(html.contains("href=\"https://rust-lang.org\""), "{html}");
        assert!(html.contains("target=\"_blank\""), "{html}");
        assert!(html.contains("rel=\"noopener noreferrer\""), "{html}");
        assert!(html.contains("title=\"the rust site\""), "{html}");
    }

    #[test]
    fn script_style_iframe_are_dropped_with_content() {
        let html = render(
            "ok\n\n<script>alert(1)</script>\n\n<style>x{}</style>\n\n<iframe src=\"https://e.com\"></iframe>\n\n<form><input></form>\n",
            0,
        );
        assert!(!html.contains("script"), "{html}");
        assert!(!html.contains("alert"), "{html}");
        assert!(!html.contains("style"), "{html}");
        assert!(!html.contains("iframe"), "{html}");
        assert!(!html.contains("<form"), "{html}");
        assert!(html.contains("ok"), "{html}");
    }

    #[test]
    fn javascript_and_data_urls_lose_their_href() {
        let html = render(
            "[x](javascript:alert(1)) [y](data:text/html;base64,PGI+)",
            0,
        );
        assert!(!html.contains("javascript"), "{html}");
        assert!(!html.contains("data:text/html"), "{html}");
        // The link text itself survives as an anchor without href.
        assert!(html.contains(">x</a>") || html.contains("x"), "{html}");
    }

    #[test]
    fn raw_html_event_handlers_are_stripped() {
        let html = render("<a href=\"https://e.com\" onclick=\"alert(1)\">e</a>", 0);
        assert!(!html.contains("onclick"), "{html}");
        // ammonia's default link_rel still forces the safe rel.
        assert!(html.contains("rel=\"noopener noreferrer\""), "{html}");
    }

    #[test]
    fn cite_anchor_placeholders_survive_sanitize() {
        // The client retargets #cite-N to #src-<turn>-<n>; the
        // placeholder must survive PassThrough handling intact.
        let html = render("[1]", 1);
        assert!(html.contains("href=\"#cite-1\""), "{html}");
    }

    #[test]
    fn empty_input_renders_empty() {
        assert_eq!(render("", 0), "");
        assert_eq!(render("   \n", 0), "");
    }

    #[test]
    fn image_markup_is_dropped_but_alt_is_not_cited() {
        let html = render("![pic [1]](https://e.com/i.png)", 1);
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("data-cite"), "{html}");
    }
}
