//! Deterministic groundedness for the `answers` cache gate (#232).
//!
//! The predicate lives in cauce-core (moved out of `cauce-agent` for
//! #234) so the eval scorer runs the *identical* function the cache
//! gate applies — calibration compares the verbalized confidence
//! against the real gate, not a reimplementation.
//!
//! Verbalized confidence — the model rating the answer it just wrote —
//! is uncalibrated, so it no longer decides what caches for 24 h (it
//! still rides `done.confidence` for display). [`groundedness`] is the
//! replacement gate: provider-agnostic, offline-testable, and it checks
//! the citation contract the system prompts actually require. An answer
//! is grounded when
//!
//! - at least one source exists,
//! - every sentence carries an in-range `[n]` citation (coverage), and
//! - every `[n]` marker resolves to a real source
//!   (`1 <= n <= sources.len()` — index validity).
//!
//! Sentence splitting is a fixed rule: `.`/`!`/`?` followed by
//! whitespace or end of line ends a sentence, except a `.` right after
//! an abbreviation or initial ("e.g.", "etc.", "U.S.", "J. Smith")
//! or inside a number ("22.5"), which never splits. Code is never
//! scanned, matching `render_answer_html`'s citation rules: fenced
//! blocks and inline code spans are blanked out first, so a `[n]`
//! inside code can neither ground a sentence nor invalidate one.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::AnswerSource;

/// Whether `answer` is grounded in `sources` — the `answers` write
/// gate. Every check is deterministic text analysis; no provider call.
pub fn groundedness(answer: &str, sources: &[AnswerSource]) -> bool {
    if sources.is_empty() {
        return false;
    }
    let n_sources = sources.len();
    let mut total = 0usize;
    let mut covered = 0usize;
    let mut indices_valid = true;
    for sentence in sentences(answer) {
        let mut cited = false;
        for n in cite_indices(&sentence) {
            if (1..=n_sources).contains(&n) {
                cited = true;
            } else {
                indices_valid = false;
            }
        }
        total += 1;
        covered += usize::from(cited);
    }
    total > 0 && covered == total && indices_valid
}

/// The prose fragments the citation scan sees: fenced code blocks and
/// inline code spans are blanked out (mirroring `render_answer_html`,
/// which never turns a `[n]` inside code into a cite anchor), then each
/// line splits at `.`/`!`/`?` followed by whitespace or end of line.
/// Fragments without an alphanumeric char drop out — a stray `**` or
/// `-` is punctuation, not a claim.
fn sentences(answer: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut line = String::new();
    for raw in answer.lines() {
        if raw.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        line.clear();
        strip_inline_code(raw, &mut line);
        split_sentences(&line, &mut out);
    }
    out
}

/// Copy `line` to `out` with `` `code` `` spans removed. A backtick run
/// opens a span closed by a run of the same length; an unmatched run is
/// literal text.
fn strip_inline_code(line: &str, out: &mut String) {
    let mut rest = line;
    while let Some(pos) = rest.find('`') {
        out.push_str(&rest[..pos]);
        let ticks = &rest[pos..];
        let run = ticks.bytes().take_while(|&b| b == b'`').count();
        let after_open = &ticks[run..];
        let needle = "`".repeat(run);
        match after_open.find(&needle) {
            Some(close) => rest = &after_open[close + run..],
            None => {
                out.push_str(&ticks[..run]);
                rest = after_open;
            }
        }
    }
    out.push_str(rest);
}

/// Words that end in a period without ending the sentence.
const ABBREVIATIONS: &[&str] = &[
    "approx", "ca", "cf", "dept", "dr", "est", "etc", "fig", "figs", "ibid", "inc", "jr", "ltd",
    "mr", "mrs", "ms", "no", "prof", "sr", "st", "viz", "vs",
];

/// Split one prose line into sentence fragments. `.`/`!`/`?` ends a
/// sentence at end of line or when followed by whitespace — except a
/// `.` whose preceding word is an abbreviation or a single letter (an
/// initial, or the tail of a dotted abbreviation like "e.g."/"U.S.").
/// Decimals never split because the `.` there is followed by a digit,
/// not whitespace.
fn split_sentences(line: &str, out: &mut Vec<String>) {
    let mut start = 0;
    for (i, c) in line.char_indices() {
        let boundary = match c {
            '.' | '!' | '?' => {
                let rest = &line[i + 1..];
                if rest.is_empty() {
                    true
                } else if !rest.starts_with(char::is_whitespace) {
                    false
                } else {
                    c != '.' || !abbrev_before(&line[..i])
                }
            }
            _ => false,
        };
        if boundary {
            push_fragment(&line[start..i], out);
            start = i + 1;
        }
    }
    push_fragment(&line[start..], out);
}

/// Whether the word ending at `before`'s end reads as an abbreviation:
/// a listed word ("etc." → "etc") or a single letter (an initial like
/// "J." — which also covers the last leg of "e.g." and "U.S.").
fn abbrev_before(before: &str) -> bool {
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    word.chars().count() == 1 || ABBREVIATIONS.contains(&word.to_lowercase().as_str())
}

fn push_fragment(frag: &str, out: &mut Vec<String>) {
    let frag = frag.trim();
    if frag.chars().any(|c| c.is_alphanumeric()) {
        out.push(frag.to_string());
    }
}

/// The `n` of every `[n]` marker in `sentence`, in order — the same
/// marker grammar as `render_answer_html`: `[` + ASCII digits + `]`.
/// An index that overflows `usize` yields `usize::MAX`, out of range
/// for any real source set.
fn cite_indices(sentence: &str) -> Vec<usize> {
    let bytes = sentence.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let digits = &sentence[i + 1..];
            let len = digits.bytes().take_while(|b| b.is_ascii_digit()).count();
            if len > 0 && digits.as_bytes().get(len) == Some(&b']') {
                out.push(digits[..len].parse().unwrap_or(usize::MAX));
                i += 1 + len + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::groundedness;
    use crate::{AnswerSource, EngineId};
    use url::Url;

    fn sources(n: usize) -> Vec<AnswerSource> {
        (0..n)
            .map(|i| AnswerSource {
                url: Url::parse(&format!("https://example.com/{i}")).unwrap(),
                title: format!("s{i}"),
                snippet: String::new(),
                engine: EngineId::from("test"),
            })
            .collect()
    }

    #[test]
    fn no_sources_is_never_grounded() {
        assert!(!groundedness("cited [1].", &sources(0)));
        assert!(!groundedness("uncited.", &sources(0)));
    }

    #[test]
    fn every_sentence_cited_is_grounded() {
        let s = sources(2);
        assert!(groundedness("one fact [1].", &s));
        assert!(groundedness("one fact [1]. Another fact [2].", &s));
        assert!(groundedness("both at once [1][2].", &s));
        assert!(groundedness("out of order [2], then [1].", &s));
    }

    #[test]
    fn an_uncovered_sentence_is_ungrounded() {
        let s = sources(2);
        assert!(!groundedness("cited claim [1]. Uncited claim.", &s));
        assert!(!groundedness("uncited lead. cited claim [1].", &s));
        // A bullet line is a sentence too.
        assert!(!groundedness("see [1].\n- a bullet", &s));
    }

    #[test]
    fn out_of_range_cite_is_ungrounded() {
        let s = sources(2);
        assert!(!groundedness("cited [1] and out of range [3].", &s));
        assert!(!groundedness("cited [0].", &s));
        assert!(!groundedness("cited [99999999999999999999].", &s));
        // [n] is the whole grammar — "[1, 2]" is not a citation.
        assert!(!groundedness("cited [1, 2].", &s));
    }

    #[test]
    fn empty_or_whitespace_answer_is_ungrounded() {
        let s = sources(1);
        assert!(!groundedness("", &s));
        assert!(!groundedness("   \n\n  ", &s));
        assert!(!groundedness("**—**", &s));
    }

    #[test]
    fn decimals_and_abbreviations_do_not_split_sentences() {
        let s = sources(1);
        assert!(groundedness("it took 3.5 hours [1].", &s));
        assert!(groundedness("sources e.g. docs confirm it [1].", &s));
        assert!(groundedness("per Dr. No's notes and J. Smith [1].", &s));
        // Case does not matter: a `. ` ends the sentence even before a
        // lowercase word, so a bare lead-in fails coverage.
        assert!(!groundedness("first. then lowercase continuation [1].", &s));
        assert!(!groundedness("first [1]. Then an uncited sentence.", &s));
    }

    #[test]
    fn code_is_never_citation_scanned() {
        let s = sources(1);
        // [n] inside a fenced block or inline span is literal text: it
        // does not cover the prose sentence, and an out-of-range index
        // there does not invalidate.
        assert!(!groundedness("the flag is `-f` per the docs.", &s));
        assert!(groundedness("the flag is `-f [9]` per the docs [1].", &s));
        assert!(groundedness(
            "per the docs [1].\n```\n[9] not a cite\n```",
            &s
        ));
        // Nor does a fenced block's own lines count as uncovered prose.
        assert!(groundedness(
            "per the docs [1].\n```rust\nlet x = 1;\n```",
            &s
        ));
    }
}
