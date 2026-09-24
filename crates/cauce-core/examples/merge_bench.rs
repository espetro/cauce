//! `merge_bench` — W3-03 review harness for the merge tuning: it replays
//! the recorded cassettes under `examples/fixtures/merge_bench/` through
//! [`RrfMerge`] twice and prints the merged top-10 before (uniform
//! weights, no host collapse) and after (reliability weights +
//! `collapse_same_host_after`), plus every URL fold `normalize_url`
//! applied (AMP unwrap, `m.`/`amp.` hosts, AMP path/param markers,
//! tracking params).
//!
//! Run: `cargo run -p cauce-core --example merge_bench`.
//!
//! The weights below are a fixed demo set standing in for live
//! [`HealthTracker::reliability`]-derived weights — the cassette engines
//! carry no health history here. "Before" is the pre-W3-03 merge: `k =
//! 60`, weight 1.0 for every engine, no per-host cap.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};

use cauce_core::{RrfMerge, SearchResult, normalize_url};
use cauce_engines::cassette::{Cassette, cassette_path};

/// The recorded query; cassettes sit at `<fixtures>/<engine>/<sha8>.json`.
const QUERY: &str = "rust ownership tutorial";

/// Cassette engines in merge order, with their demo reliability weights
/// (the W3-03 range is (0.5, 1.0]): `brave` stands in for a degraded
/// engine so its folded AMP/mobile duplicates contribute less, `kagi`
/// for a middling one.
const ENGINES: &[(&str, f32)] = &[("bing", 1.0), ("brave", 0.5), ("kagi", 0.8)];

/// "After" merge settings — the `MergePolicy` defaults.
const RRF_K: f32 = 60.0;
const COLLAPSE_AFTER: usize = 3;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures/merge_bench")
}

fn merge(
    pages: &[Vec<SearchResult>],
    k: f32,
    weights: &[f32],
    collapse: usize,
) -> Vec<SearchResult> {
    let mut merge = RrfMerge::new(k, weights.to_vec(), collapse);
    for (idx, page) in pages.iter().enumerate() {
        merge.add(idx, page);
    }
    merge.finish()
}

fn print_table(label: &str, merged: &[SearchResult]) {
    println!("\n== {label} ({} results) ==", merged.len());
    for (i, r) in merged.iter().take(10).enumerate() {
        println!("{:>2}. score={:.5}  {}", i + 1, r.score, r.url);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = fixtures_root();
    println!(
        "merge_bench: query {QUERY:?}, fixtures at {}",
        root.display()
    );

    let mut pages: Vec<Vec<SearchResult>> = Vec::with_capacity(ENGINES.len());
    for (engine, _) in ENGINES {
        let path = cassette_path(&root, engine, QUERY);
        let cassette = Cassette::load(&path).map_err(|e| format!("{e} ({})", path.display()))?;
        println!("\n-- {engine}: {} cassette rows", cassette.results.len());
        let mut folded = 0usize;
        let page: Vec<SearchResult> = cassette
            .results
            .iter()
            .map(|r| {
                // Engines hand the pipeline the normalized URL already
                // (declarative parse stores it); keep that boundary so
                // the merge sees what production sees.
                let normalized = normalize_url(&r.url);
                if normalized != r.url {
                    folded += 1;
                    println!("   fold {} -> {}", r.url, normalized);
                }
                SearchResult {
                    url: normalized,
                    ..r.clone()
                }
            })
            .collect();
        println!("   {folded} folded");
        pages.push(page);
    }
    let raw: usize = pages.iter().map(Vec::len).sum();
    println!("\n{raw} raw rows across {} engines", ENGINES.len());

    let before = merge(&pages, RRF_K, &vec![1.0; ENGINES.len()], 0);
    let weights: Vec<f32> = ENGINES.iter().map(|(_, w)| *w).collect();
    let after = merge(&pages, RRF_K, &weights, COLLAPSE_AFTER);

    print_table("before: uniform weight 1.0, no collapse", &before);
    print_table(
        &format!("after: weights {weights:?}, collapse_same_host_after={COLLAPSE_AFTER}"),
        &after,
    );
    Ok(())
}
