//! RRF merge of engine outcomes over normalised URLs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashMap;

use tracing::{debug, info_span};
use url::Url;
use uuid::Uuid;

use crate::normalize::normalize_url;
use crate::response::SearchResult;

use super::fanout::FanOut;
use super::waves::Gated;
use super::*;

/// RRF constant (parent plan 4.4 step 5).
pub(super) const RRF_K: f32 = 60.0;

/// Incremental RRF accumulator. Contributions are keyed by engine index so
/// out-of-order completion never changes floating-point reduction or ties.
pub(super) struct RrfMerge {
    pub(super) map: HashMap<Url, RrfAcc>,
    pub(super) raw_count: usize,
}

pub(super) struct RrfAcc {
    pub(super) result: SearchResult,
    pub(super) contributions: std::collections::BTreeMap<usize, f32>,
    pub(super) best: f32,
    pub(super) best_order: (usize, usize),
    pub(super) first_seen: (usize, usize),
}

impl RrfMerge {
    pub(super) fn new() -> Self {
        Self {
            map: HashMap::new(),
            raw_count: 0,
        }
    }

    pub(super) fn add(&mut self, engine_idx: usize, results: &[SearchResult]) {
        self.raw_count += results.len();
        for (rank0, result) in results.iter().enumerate() {
            let rank = rank0 + 1;
            let contribution = 1.0 / (RRF_K + rank0 as f32 + 1.0);
            // Dedupe on the normalized form but emit the engine's raw URL.
            let normalized = normalize_url(&result.url);
            let acc = self.map.entry(normalized).or_insert_with(|| RrfAcc {
                result: result.clone(),
                contributions: std::collections::BTreeMap::new(),
                best: contribution,
                best_order: (engine_idx, rank),
                first_seen: (engine_idx, rank),
            });
            *acc.contributions.entry(engine_idx).or_default() += contribution;
            acc.first_seen = acc.first_seen.min((engine_idx, rank));
            if contribution > acc.best
                || (contribution == acc.best && (engine_idx, rank) < acc.best_order)
            {
                acc.best = contribution;
                acc.best_order = (engine_idx, rank);
                acc.result.clone_from(result);
            }
        }
    }

    pub(super) fn finish(self) -> Vec<SearchResult> {
        let mut merged: Vec<(f32, (usize, usize), SearchResult)> = self
            .map
            .into_values()
            .map(|acc| {
                let score = acc.contributions.values().copied().sum();
                (
                    score,
                    acc.first_seen,
                    SearchResult {
                        score,
                        ..acc.result
                    },
                )
            })
            .collect();
        merged.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        merged.into_iter().map(|(_, _, result)| result).collect()
    }
}

impl SearchPipeline {
    /// Final merge for the fan-out: fill the slots whose tasks never
    /// answered (a `JoinError` — panic/cancel — never names its engine),
    /// then the incremental [`RrfMerge`] finishes dedupe-by-URL + RRF.
    pub(super) fn merge_outcomes(
        &self,
        gated: &[Gated],
        fan: &mut FanOut,
        merge: RrfMerge,
        request_id: Uuid,
    ) -> Vec<SearchResult> {
        self.reconcile_unanswered(gated, fan, request_id);
        let raw = merge.raw_count;
        let span = info_span!(
            "merge",
            request_id = %request_id,
            r#in = tracing::field::Empty,
            out = tracing::field::Empty,
            deadline_hit = fan.deadline_hit,
        );
        let _e = span.enter();
        let merged = merge.finish();
        span.record("in", raw as u64);
        span.record("out", merged.len() as u64);
        debug!(
            raw,
            merged = merged.len(),
            deadline_hit = fan.deadline_hit,
            "merged results"
        );
        merged
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn result(idx: usize, url: &str) -> SearchResult {
        SearchResult {
            url: Url::parse(url).unwrap(),
            title: format!("t{idx}"),
            snippet: String::new(),
            engine: EngineId::from(format!("e{idx}").as_str()),
            published: None,
            score: 0.0,
        }
    }

    fn run(batches: &[(usize, Vec<SearchResult>)]) -> Vec<SearchResult> {
        let mut merge = RrfMerge::new();
        for (idx, results) in batches {
            merge.add(*idx, results);
        }
        merge.finish()
    }

    proptest! {
        /// The merged ranking depends on the configured engine order, not
        /// the completion order: `first_seen`/`best_order` are
        /// `(engine_idx, rank)` keys, so permuting which batch `add`s
        /// first must not change the output (the W3-03 order-stability
        /// contract, pinned here shift-left at the `RrfMerge` boundary).
        #[test]
        fn rrf_merge_is_completion_order_independent(
            batches in prop::collection::vec(
                prop::collection::vec("https?://[a-z]{2,8}\\.[a-z]{2,3}/[a-z0-9]{0,12}", 0..6),
                1..5,
            ),
            rot in 0..11usize,
        ) {
            let canonical: Vec<(usize, Vec<SearchResult>)> = batches
                .iter()
                .enumerate()
                .map(|(i, urls)| (i, urls.iter().map(|u| result(i, u)).collect()))
                .collect();
            let mut reversed = canonical.clone();
            reversed.reverse();
            let rotated: Vec<(usize, Vec<SearchResult>)> = canonical
                .iter()
                .cycle()
                .skip(rot % canonical.len())
                .take(canonical.len())
                .cloned()
                .collect();
            let want = run(&canonical);
            prop_assert_eq!(&want, &run(&reversed));
            prop_assert_eq!(&want, &run(&rotated));
        }
    }
}
