# W3: tail tolerance

Iteration 4 (2026-11-03 to 2026-11-16). Priority P1. EPIC issue: #4.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-2-ui-and-observability.md`.

## Goal

The scheduler stops treating every engine equally: P90-triggered hedging to tier 2,
stale-while-revalidate, and a nightly signal that tells us when a
parser silently degrades. This is the wave that makes cauce measurably different from SearXNG
on latency and reliability, and every knob is configurable because the owner's numbers
(Bing 1.8-3.1 s, Brave 0.4-0.5 s) will not be everyone's.

## Settled inputs

- Parent 4.4 scheduler description. Defaults: hard deadline 3000 ms, hedge trigger =
  P90 of tier-1 EWMA history with floor 300 ms and ceiling 1500 ms, `min_results` 5. All
  under `[search]` in config and visible in `/settings`.
- Tier-2 engines: `ddgs` exec bridge (default), plus any declarative spec with `tier: 2`
  (Yahoo, Startpage candidates; specs are follow-ups, not steps here).
- Evals live in `evals/engines/*.jsonl`, format
  `{"query": "...", "expect_domains_top5": ["tanstack.com"], "engines": ["bing","brave"]}`.
  Nightly only; never gates merges.

## Exit criteria

1. On replay with tier-1 `latency_ms=2000` and tier-2 `latency_ms=200`, TTFR < 600 ms
   and `meta.hedged=true`.
2. Nightly relevance eval publishes a JSON artifact and a dashboard panel reads its last
   run.
3. Removed: per-client fairness is deferred to `later/per-client-fairness.md` (issue #47).
4. A permanently-broken engine rests (breaker on Parse/Transport streak) instead of
   burning deadline latency forever.

## Steps

### W3-01 P90 hedge to tier 2
- Issue #44 · Effort M · Label feature · Team Systems · Branch `v3/w3-01-hedge`
- Depends on: W2-01 to W2-09
- Do: scheduler keeps a rolling latency histogram per engine (HDR-style, in
  `EngineHealth`); at `t = clamp(P90(tier1), floor, ceiling)` if fewer than `min_results`
  merged results have arrived, fire healthy tier-2 engines with the remaining budget;
  `meta.hedged`, `meta.hedge_at_ms`; metric `cauce_hedge_total{reason=slow|few}`; settings
  fields wired.
- Acceptance: exit criterion 1; with tier-1 fast (`latency_ms=100`, 10 results) tier 2 is
  never called.
- Follow-up: W3-02.
- Amended 2026-09-23: dependency was W2-10. The step is verified on `replay` and tunes
  against latency numbers already in "Settled inputs"; the usage week (W2-10, #42) is
  calendar-bound and adds W3 steps rather than feeding this one, so it runs alongside W3 and
  gates W4-01 instead (issue #42 comment).

### W3-02 Stale-while-revalidate
- Issue #45 · Effort S · Label feature · Team Systems · Branch `v3/w3-02-swr`
- Depends on: W3-01
- Do: expired tier-1 rows within `cache.stale_grace_s` (default 6 h) are served immediately
  as `Source::Cache{stale:true}` while a background refresh runs (deduped by singleflight);
  `evict_expired` respects the grace window; metric `cauce_stale_served_total{reason=grace|
  admission}`; the UI badge says `stale · refreshing`; three cache-hygiene rules: (a) never
  `put` a response whose `results` is empty (an all-NoResults fan-out must not be cached —
  a wedged exec engine returning `[]` would otherwise poison the cache for the full TTL);
  (b) `cache.degraded_ttl_s` (default 60) applies when any engine `Failed` or
  `deadline_hit` — a partial response doesn't earn the full TTL; (c) when a stale row is
  served while ALL pinned engines are unhealthy (breaker open or in `skipped`), emit a warn
  event and bump `cauce_stale_served_total{reason=engines_unhealthy}` so 'everything is down,
  serving only stale' is an aggregate signal, not a per-response footnote.
- Acceptance: expire a row in the temp DB, search returns stale in < 5 ms, a second search
  200 ms later returns fresh `Network`-derived cache; a replay engine returning `[]`
  produces a 200 with no cache write; a stale serve during a simulated all-breaker outage
  logs the warn event and increments the labeled counter.
- Follow-up: W3-03.

### W3-03 RRF and URL normalisation tuning
- Issue #46 · Effort S · Label feature · Team Systems · Branch `v3/w3-03-merge-tuning`
- Depends on: W3-01
- Do: RRF k configurable; engine weight from reliability (0.5 to 1.0); host-level dedupe
  option (`merge.collapse_same_host_after`, default 3); `normalize_url` gains AMP unwrapping
  and `m.` host folding; a `merge_bench` example with recorded cassettes printing the top-10
  before/after for review.
- Acceptance: property test that RRF is order-stable across engine arrival permutations;
  fixture asserting AMP URL folds to canonical.
- Follow-up: W3-05.

### W3-05 Engine relevance evals (nightly)
- Issue #48 · Effort M · Label infra · Team Systems · Branch `v3/w3-05-relevance-evals`
- Depends on: W3-03
- Do: `cauce eval engines evals/engines/*.jsonl --live` runs each case against the named
  engines (politeness applies), scores domain-hit@5 per engine, writes
  `evals/results/<date>-engines.json`; nightly workflow runs it with 15 cases total (5 per
  engine) and uploads the artifact; `/api/stats` exposes the last run when the file exists;
  dashboard panel "engine relevance (nightly)". Threshold in one file (`evals/thresholds.toml`);
  below threshold the workflow opens/updates a single tracking issue, never fails merges.
- Acceptance: `cauce eval engines` on replay cassettes scores 100 % for a case whose cassette
  contains the expected domain; the workflow file exists and is `cron` only.
- Follow-up: W3-06.

### W3-06 `--live` canary (nightly)
- Issue #49 · Effort S · Label infra · Team Systems · Branch `v3/w3-06-drift-canary`
- Depends on: W3-05
- Do: nightly `cauce engine test --live` for every shipped spec: fetch once, parse, exit
  non-zero on zero results, `Parse` errors, or field fill-rate < 50 % versus the committed
  fixture. The failed nightly run is the report; no markdown drift-report generator and no
  tracking-issue automation. The `--live` canary additionally fetches `page=2` for each
  spec and fails when normalized-URL overlap with page 1 exceeds ~70% (the 'engine
  silently serves page 1 again' anti-bot pattern, SearXNG #3402/#4546) or when the page-1
  result count drops below ~50% of the committed fixture (partial selector drift, SearXNG
  #4910).
- Acceptance: a failing canary exits non-zero in a test with a fixture that has a removed
  selector.
- Follow-up: W4-01.

### W3-07 Breaker on consecutive Parse/Transport failures
- Issue #119 · Effort M · Label infra · Team Systems · Branch `v3/w3-07-degraded-breaker`
- Depends on: W1-06
- Do: extend `HealthPolicy` with `degraded_threshold` (default 5) and `degraded_window`
  (default 600 s): N consecutive `Parse` or `Transport` errors within the window open the
  breaker like the timeout streak does. Today a drifted selector or a captcha page that
  evades `detect.blocked` fails on every request forever without ever being rested — worse
  for tail latency than a skipped engine and it spams the upstream during an active block.
  `NoResults`/successful answers reset the streak.
- Acceptance: a replay engine scripted to `Parse` 5 times in a row opens the breaker on the
  5th; a `Parse` streak interrupted by an `Ok` or `NoResults` does not.
- Follow-up: W2-05 surfaces the new streak counts.

### W3-08 Pipeline and test-surface decomposition
- Issue #151 · Effort M · Label infra · Team Systems · Branch `v3/w3-08-pipeline-decomp`
- Depends on: W3-07
- Do: architecture-hygiene checkpoint before W4 adds AI streaming, tool loops, and
  provider-specific code. `crates/cauce-core/src/pipeline.rs` (~2.5k LOC) now carries
  request/cache orchestration, lexical lookup, admission, breaker gating, wave
  construction, hedge timing and permit races, engine fan-out, outcome folding, merge,
  persistence, and both streaming paths — split it along privacy boundaries
  (`pipeline/mod.rs` for the public `SearchPipeline` API and orchestration, plus focused
  files for cache lookup, wave gating, fan-out/hedge, outcome folding, merge, and
  persistence); split `tests/support/mod.rs` into focused fixture modules; add a
  non-gating LOC/complexity report (implementation vs test LOC, largest files/functions,
  changed LOC per PR) with warning thresholds (file > 1000 review, > 1500 decomposition
  issue, fn > 150 extract, test module > 800 split) — report only, never fails merges;
  adopt the test-shape guidance where it removes repetition: table-driven case tables
  for repeated config/outcome combinations, proptest for invariants (RRF order-stability,
  URL-normalisation idempotence, permit acquisition never leaks, budgets never negative),
  fixture directories for parser cases, fuzzing only on parser/security boundaries
  (URL normalisation, selectors, SSE frame assembly), and shift-left contract tests at
  the new module boundaries so W4 does not grow `pipeline.rs` again. The target is lower
  coupling and easier review, not a per-file LOC cap.
- Acceptance: `pipeline.rs` decomposed into a `pipeline/` module directory with no
  behaviour change (the full workspace suite passes untouched); the LOC report runs in
  CI and publishes its numbers without failing; at least one repeated test cluster
  converted to a case table.
- Follow-up: W4 steps build on the new module boundaries.

## Out of scope for W3

New engine specs (file as follow-ups with the failing canary as evidence), per-client
fairness (`later/per-client-fairness.md`), AI, archive.

## Follow-up

W4 `wave-4-ai-mode.md`. Candidate tier-2 specs (`yahoo.yaml`, `startpage.yaml`) are
`later/` items until a real need shows up in the relevance evals.
