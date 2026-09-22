# W3: tail tolerance

Iteration 4 (2026-11-03 to 2026-11-16). Priority P1. EPIC issue: #4.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-2-ui-and-observability.md`.

## Goal

The scheduler stops treating every engine equally: P90-triggered hedging to tier 2,
stale-while-revalidate, and a nightly signal that tells us when a
parser silently degrades. This is the wave that makes oxe measurably different from SearXNG
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

## Steps

### W3-01 P90 hedge to tier 2
- Issue #44 · Effort M · Label feature · Team Systems · Branch `v3/w3-01-hedge`
- Depends on: W2-10
- Do: scheduler keeps a rolling latency histogram per engine (HDR-style, in
  `EngineHealth`); at `t = clamp(P90(tier1), floor, ceiling)` if fewer than `min_results`
  merged results have arrived, fire healthy tier-2 engines with the remaining budget;
  `meta.hedged`, `meta.hedge_at_ms`; metric `oxe_hedge_total{reason=slow|few}`; settings
  fields wired.
- Acceptance: exit criterion 1; with tier-1 fast (`latency_ms=100`, 10 results) tier 2 is
  never called.
- Follow-up: W3-02.

### W3-02 Stale-while-revalidate
- Issue #45 · Effort S · Label feature · Team Systems · Branch `v3/w3-02-swr`
- Depends on: W3-01
- Do: expired tier-1 rows within `cache.stale_grace_s` (default 6 h) are served immediately
  as `Source::Cache{stale:true}` while a background refresh runs (deduped by singleflight);
  `evict_expired` respects the grace window; metric `oxe_stale_served_total{reason=grace|
  admission}`; the UI badge says `stale · refreshing`.
- Acceptance: expire a row in the temp DB, search returns stale in < 5 ms, a second search
  200 ms later returns fresh `Network`-derived cache.
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
- Do: `oxe eval engines evals/engines/*.jsonl --live` runs each case against the named
  engines (politeness applies), scores domain-hit@5 per engine, writes
  `evals/results/<date>-engines.json`; nightly workflow runs it with 15 cases total (5 per
  engine) and uploads the artifact; `/api/stats` exposes the last run when the file exists;
  dashboard panel "engine relevance (nightly)". Threshold in one file (`evals/thresholds.toml`);
  below threshold the workflow opens/updates a single tracking issue, never fails merges.
- Acceptance: `oxe eval engines` on replay cassettes scores 100 % for a case whose cassette
  contains the expected domain; the workflow file exists and is `cron` only.
- Follow-up: W3-06.

### W3-06 `--live` canary (nightly)
- Issue #49 · Effort S · Label infra · Team Systems · Branch `v3/w3-06-drift-canary`
- Depends on: W3-05
- Do: nightly `oxe engine test --live` for every shipped spec: fetch once, parse, exit
  non-zero on zero results, `Parse` errors, or field fill-rate < 50 % versus the committed
  fixture. The failed nightly run is the report; no markdown drift-report generator and no
  tracking-issue automation.
- Acceptance: a failing canary exits non-zero in a test with a fixture that has a removed
  selector.
- Follow-up: W4-01.

## Out of scope for W3

New engine specs (file as follow-ups with the failing canary as evidence), per-client
fairness (`later/per-client-fairness.md`), AI, archive.

## Follow-up

W4 `wave-4-ai-mode.md`. Candidate tier-2 specs (`yahoo.yaml`, `startpage.yaml`) are
`later/` items until a real need shows up in the relevance evals.
