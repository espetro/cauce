//! Fixture test for the W3-03 URL folds: every `in`/`want` pair in
//! `tests/fixtures/normalize_url_folds.json` asserts that
//! [`normalize_url`] maps an AMP wrapper, `m.`/`amp.` host or AMP
//! marker onto its canonical form (the `folds` list), or deliberately
//! leaves a lookalike alone (`kept` — an `amps` label, a non-terminal
//! `/amp/` segment, a non-amp `outputType`).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::normalize_url;
use url::Url;

#[derive(serde::Deserialize)]
struct Fold {
    /// Raw URL as an engine would emit it.
    #[serde(rename = "in")]
    input: String,
    /// Canonical URL the merge's dedupe key must be.
    want: String,
    /// Human-readable rule being pinned (assertion context only).
    #[allow(dead_code)]
    why: String,
}

#[derive(serde::Deserialize)]
struct Fixture {
    folds: Vec<Fold>,
    kept: Vec<Fold>,
}

fn load() -> Fixture {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/normalize_url_folds.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture reads"))
        .expect("fixture parses")
}

/// Every `folds` entry normalizes to `want` — the AMP-fold acceptance
/// (issue #46).
#[test]
fn amp_urls_fold_to_canonical() {
    for fold in load().folds {
        let url = Url::parse(&fold.input).expect("fixture input parses");
        let got = normalize_url(&url);
        assert_eq!(
            got.as_str(),
            fold.want,
            "{} should fold to {} ({})",
            fold.input,
            fold.want,
            fold.why,
        );
        // The fold is terminal: a second pass changes nothing.
        assert_eq!(
            normalize_url(&got),
            got,
            "fold for {} is idempotent",
            fold.input
        );
    }
}

/// Lookalikes that must NOT fold — the fixture pins the negative space
/// so a future rule can't over-collapse.
#[test]
fn amp_lookalikes_survive() {
    for fold in load().kept {
        let url = Url::parse(&fold.input).expect("fixture input parses");
        let got = normalize_url(&url);
        assert_eq!(
            got.as_str(),
            fold.want,
            "{} must not fold ({})",
            fold.input,
            fold.why,
        );
    }
}
