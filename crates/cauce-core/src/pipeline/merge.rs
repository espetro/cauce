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

/// Default RRF constant (parent plan 4.4 step 5; `merge.rrf_k`, W3-03).
pub const DEFAULT_RRF_K: f32 = 60.0;

/// Default cap on merged results emitted per host — the
/// `merge.collapse_same_host_after` default (W3-03).
pub const DEFAULT_COLLAPSE_SAME_HOST_AFTER: usize = 3;

/// Incremental RRF accumulator. Contributions are keyed by engine index so
/// out-of-order completion never changes floating-point reduction or ties;
/// the per-engine `weights` snapshot (reliability, W3-03) is fixed at
/// construction for the same reason.
pub struct RrfMerge {
    pub(super) map: HashMap<Url, RrfAcc>,
    pub(super) raw_count: usize,
    /// `k` in `score += weight / (k + rank)`.
    k: f32,
    /// Per-engine reliability weight in `(0.5, 1.0]`, indexed like
    /// [`add`]'s `engine_idx`; out-of-range indices get full weight.
    ///
    /// [`add`]: Self::add
    weights: Vec<f32>,
    /// Max results emitted per host; `0` disables the collapse.
    collapse_same_host_after: usize,
}

pub(super) struct RrfAcc {
    pub(super) result: SearchResult,
    pub(super) contributions: std::collections::BTreeMap<usize, f32>,
    pub(super) best: f32,
    pub(super) best_order: (usize, usize),
    pub(super) first_seen: (usize, usize),
}

impl RrfMerge {
    /// A merge with the given RRF `k`, per-engine weights and per-host
    /// emission cap (see the field docs). `weights` must be indexed the
    /// same way `add` is called — the request's runnable list.
    pub fn new(k: f32, weights: Vec<f32>, collapse_same_host_after: usize) -> Self {
        Self {
            map: HashMap::new(),
            raw_count: 0,
            k,
            weights,
            collapse_same_host_after,
        }
    }

    pub fn add(&mut self, engine_idx: usize, results: &[SearchResult]) {
        self.raw_count += results.len();
        let weight = self.weights.get(engine_idx).copied().unwrap_or(1.0);
        for (rank0, result) in results.iter().enumerate() {
            let rank = rank0 + 1;
            let contribution = weight / (self.k + rank0 as f32 + 1.0);
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

    /// Emit the merged results, best RRF score first, then keep at most
    /// `collapse_same_host_after` per host — counted on the *normalised*
    /// host, so `m.`/`amp.` folds collapse together (`0` emits all).
    pub fn finish(self) -> Vec<SearchResult> {
        let mut merged: Vec<(f32, (usize, usize), Url, SearchResult)> = self
            .map
            .into_iter()
            .map(|(key, acc)| {
                let score = acc.contributions.values().copied().sum();
                (
                    score,
                    acc.first_seen,
                    key,
                    SearchResult {
                        score,
                        ..acc.result
                    },
                )
            })
            .collect();
        merged.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let cap = self.collapse_same_host_after;
        let mut per_host: HashMap<String, usize> = HashMap::new();
        merged
            .into_iter()
            .filter(|(_, _, key, _)| {
                if cap == 0 {
                    return true;
                }
                let n = per_host
                    .entry(key.host_str().unwrap_or_default().to_string())
                    .or_insert(0);
                if *n >= cap {
                    return false;
                }
                *n += 1;
                true
            })
            .map(|(_, _, _, result)| result)
            .collect()
    }
}

impl SearchPipeline {
    /// Per-engine RRF weights for this flight (W3-03): observed
    /// reliability scaled into `(0.5, 1.0]` — `0.5 + 0.5 * reliability`
    /// where reliability is `answered / samples` (full trust until the
    /// engine has recorded calls; see [`HealthTracker::reliability`]).
    /// Snapshotted once before fan-out so a mid-flight health update can
    /// never reorder contributions by completion order.
    ///
    /// [`HealthTracker::reliability`]: crate::health::HealthTracker::reliability
    pub(super) fn merge_weights(&self, runnable: &[Arc<dyn Engine>]) -> Vec<f32> {
        runnable
            .iter()
            .map(|engine| 0.5 + 0.5 * self.health.reliability(&engine.id()) as f32)
            .collect()
    }

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

    fn run(
        k: f32,
        weights: &[f32],
        cap: usize,
        batches: &[(usize, Vec<SearchResult>)],
    ) -> Vec<SearchResult> {
        let mut merge = RrfMerge::new(k, weights.to_vec(), cap);
        for (idx, results) in batches {
            merge.add(*idx, results);
        }
        merge.finish()
    }

    /// `w(1) / (w(1) + w(0))` rank-1 ties break on the lower engine idx,
    /// weight or not; rank-2 from the weaker engine then beats rank-1.
    #[test]
    fn reliability_weight_reorders_rank_ties() {
        let shared = "https://example.com/x";
        let weak = "https://weak.example.com/x";
        let strong = "https://strong.example.com/x";
        // Engine 0 is less reliable (0.5): its rank-1 loses to engine 1's
        // rank-1 despite the lower first_seen index.
        let batches: Vec<(usize, Vec<SearchResult>)> =
            vec![(0, vec![result(0, weak)]), (1, vec![result(1, strong)])];
        let weighted = run(60.0, &[0.5, 1.0], 0, &batches);
        assert_eq!(weighted[0].url.as_str(), strong);
        let even = run(60.0, &[1.0, 1.0], 0, &batches);
        assert_eq!(even[0].url.as_str(), weak);
        // Weighting scales but never zeroes a contribution (range (0.5, 1.0]):
        // the weak engine's rank-1 still beats its own rank-2.
        let merged = run(
            60.0,
            &[0.5, 1.0],
            0,
            &[
                (0, vec![result(0, shared), result(0, weak)]),
                (1, vec![result(1, strong)]),
            ],
        );
        assert_eq!(merged[0].url.as_str(), strong);
        assert_eq!(merged[1].url.as_str(), shared);
        assert_eq!(merged[2].url.as_str(), weak);
    }

    /// `collapse_same_host_after` caps emissions per host on the merged
    /// order; `0` disables the cap.
    #[test]
    fn collapse_same_host_after_caps_per_host() {
        let urls: Vec<SearchResult> = (0..5)
            .map(|i| result(0, &format!("https://dup.example.com/{i}")))
            .collect();
        let capped = run(60.0, &[1.0], 3, &[(0, urls.clone())]);
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[0].url.as_str(), "https://dup.example.com/0");
        // The cap counts the normalised host: `m.` folds collapse into it.
        let mixed = vec![
            result(0, "https://m.dup.example.com/a"),
            result(0, "https://dup.example.com/b"),
            result(0, "https://dup.example.com/c"),
            result(0, "https://dup.example.com/d"),
        ];
        assert_eq!(run(60.0, &[1.0], 3, &[(0, mixed)]).len(), 3);
        let open = run(60.0, &[1.0], 0, &[(0, urls)]);
        assert_eq!(open.len(), 5);
    }

    /// The RRF `k` is the constructor's, not a baked constant.
    #[test]
    fn rrf_k_is_configurable() {
        let one = vec![result(0, "https://a.example.com/")];
        let flat = run(1.0, &[1.0], 0, &[(0, one.clone())]);
        assert!((flat[0].score - 0.5).abs() < 1e-6, "k=1 rank-1 -> 1/2");
        let std = run(60.0, &[1.0], 0, &[(0, one)]);
        assert!((std[0].score - 1.0 / 61.0).abs() < 1e-6);
    }

    proptest! {
        /// The merged ranking depends on the configured engine order, not
        /// the completion order: `first_seen`/`best_order` are
        /// `(engine_idx, rank)` keys and `weights`/`k` are fixed at
        /// construction, so permuting which batch `add`s first must not
        /// change the output (the W3-03 order-stability contract, pinned
        /// here shift-left at the `RrfMerge` boundary). The weights axis
        /// is generated too: any fixed weight vector keeps the property.
        #[test]
        fn rrf_merge_is_completion_order_independent(
            batches in prop::collection::vec(
                prop::collection::vec("https?://[a-z]{2,8}\\.[a-z]{2,3}/[a-z0-9]{0,12}", 0..6),
                1..5,
            ),
            weights in prop::collection::vec(0.5f32..=1.0f32, 1..5),
            k in 1.0f32..=120.0f32,
            cap in 0usize..4,
            rot in 0..11usize,
        ) {
            let canonical: Vec<(usize, Vec<SearchResult>)> = batches
                .iter()
                .enumerate()
                .map(|(i, urls)| (i, urls.iter().map(|u| result(i, u)).collect()))
                .collect();
            let n = canonical.len();
            let weights: Vec<f32> = weights.iter().copied().cycle().take(n).collect();
            let mut reversed = canonical.clone();
            reversed.reverse();
            let rotated: Vec<(usize, Vec<SearchResult>)> = canonical
                .iter()
                .cycle()
                .skip(rot % n)
                .take(n)
                .cloned()
                .collect();
            let want = run(k, &weights, cap, &canonical);
            prop_assert_eq!(&want, &run(k, &weights, cap, &reversed));
            prop_assert_eq!(&want, &run(k, &weights, cap, &rotated));
        }
    }
}
