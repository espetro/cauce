# W3-03 RRF and URL normalisation tuning (2026-09-24)

Findings from implementing issue #46 (`v3/w3-03-merge-tuning`).

## Reliability weight source: `HealthTracker`, not the metrics registry

The plan left the reliability source open ("read what's available on
`EngineHealth`/the metric stats"). `engine_stats()`' `reliability_pct` is
process-global — a weight computed from it would leak across pipelines in
integration tests. `EngineHealth` got a private `answered` counter
(incremented in `record_ok`, covering `Ok` + `NoResults`) and
`HealthTracker::reliability(id)` = `answered / samples`, `1.0` with no
calls. The merge maps it into the plan's (0.5, 1.0] range as
`0.5 + 0.5 * reliability`, snapshotted per flight at `RrfMerge`
construction — a mid-flight `record_*` can never reorder contributions
after the fact, which is what keeps
`rrf_merge_is_completion_order_independent` honest.

## `collapse_same_host_after` counts the *normalised* host

`RrfMerge::finish` caps emitted rows per `normalize_url` host, so `m.`/
`amp.` folds share the budget with the canonical host (`0` disables).
Consequence worth remembering when tuning: `doc.rust-lang.org` legitimately
places 4+ deep results for doc queries — at the default cap 3 the 4th
drops even when it's a different page. The `merge_bench` output shows this
live (the `nomicon` hit collapses out of the after-list).

## `normalize_url` ordering rules

- The AMP viewer unwrap (`*.cdn.ampproject.org` `/c/[s]/`,
  `*.google.com/amp/s/`) runs FIRST: the embedded host+path then goes
  through every other rule (`m.` folds, param/path markers).
- Trailing-slash strip runs BEFORE the `/amp` tail check, or
  `/story/amp/` misses the fold.
- Wrapper queries are dropped wholesale — `amp_gsa`/`amp_r` viewer params
  would otherwise ride along on the canonical page.
- `m.`/`amp.` host folds loop so `m.amp.x` and `amp.m.x` both collapse.

## `RrfMerge` is now `pub` for `merge_bench`

`merge_bench` lives at `crates/cauce-core/examples/merge_bench.rs` — the
`cauce-engines` cyclic dev-dep already exists for pipeline tests, so the
example can use real `Cassette`/`cassette_path`. Cassettes are
hand-authored at `crates/cauce-core/examples/fixtures/merge_bench/`
(engine-dir + `<sha8(normalized_query)>.json` naming, same as the replay
fixtures). The bench normalizes cassette URLs at load to mirror the
production boundary — `parse.rs`/`replay.rs` already store
`normalize_url(url)` into `SearchResult.url`, so the merge sees the folded
form either way; the "before" run only differs in weights/cap, not in
normalization.

## Env overrides for merge knobs

`CAUCE_MERGE_RRF_K` / `CAUCE_MERGE_COLLAPSE_SAME_HOST_AFTER`, `parse=true`,
plus a `from_raw` guard rejecting non-finite or negative `rrf_k` (a
negative `k` zeroes `k + rank` at `rank == -k - 1`).
