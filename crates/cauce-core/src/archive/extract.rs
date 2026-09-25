//! Readability extraction and HTML->markdown conversion (W5-01).
//!
//! [`dom_smoothie`] is a pure-Rust port of Mozilla's Readability.js and was
//! chosen over `readability-rs` on the 10-fixture corpus: it keeps
//! multi-block content (every post of a forum thread, an essay's footnote
//! section) that `readability-rs` drops to its top candidate. Extraction
//! runs in the default `Raw` text mode (cleaned article HTML); conversion
//! is [`htmd`], which emits cleaner markdown than `dom_smoothie`'s own
//! `TextMode::Markdown` (that mode escapes `.` and `"` mid-prose).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// What [`extract`] produced: the readability title plus the markdown body.
#[derive(Debug)]
pub struct Extracted {
    /// Article title as readability resolved it; may be empty.
    pub title: String,
    /// Article body as markdown.
    pub markdown: String,
}

/// Extract the article out of `html` and convert it to markdown.
///
/// `url` is the page's own URL so relative links/images resolve in the
/// markdown. Returns an error string when readability cannot score a
/// candidate or the conversion fails.
pub fn extract(html: &str, url: &str) -> Result<Extracted, String> {
    let mut readability =
        dom_smoothie::Readability::new(html, Some(url), None).map_err(|e| e.to_string())?;
    let article = readability.parse().map_err(|e| e.to_string())?;
    let markdown = htmd::convert(&article.content).map_err(|e| e.to_string())?;
    Ok(Extracted {
        title: article.title,
        markdown,
    })
}
