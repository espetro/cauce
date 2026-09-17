# tests/ rules

## Taxonomy

- **Unit.** Pure-function and module-level tests, no network, no real filesystem outside the
  redirected tmp dirs `tests/conftest.py` sets up. Today's `tests/test_health.py` is this tier.
- **Property.** `hypothesis`-driven tests pinning invariants (e.g. the Exa round-trip, cache-key
  stability under reordering) rather than example-by-example assertions. Not landed yet — wave 2.
- **Contract.** `schemathesis` run against the generated `openapi.json`, checking the live app
  against its own published spec in both directions. Not landed yet — wave 2.
- **e2e.** Playwright plus a markdown-file adapter, driven by URL deep links, under `tests/e2e/`.
  One checkpoint file per screen-spec checkpoint. `tests/e2e/` does not exist yet — a parallel
  task is doing the screens surgery on `.agents/docs/screens/` that will populate it.

## Enforced

- Python tests run via `pytest`, with `ResourceWarning` promoted to a hard error and
  `pytest-randomly` randomising test order (so order-dependent tests surface instead of going
  flaky in CI later). [gate: test:py]
- `tests/conftest.py` redirects `OXE_CACHE_DIR` and `OXE_CONFIG_DIR` to a tmp path before any
  `oxe` module import, so no test touches a real user cache/config dir. [gate: tests/conftest.py]

## Conventions

- A screen spec under `.agents/docs/screens/` without a matching e2e checkpoint file under
  `tests/e2e/` is incomplete. **Not gated yet**: `tests/e2e/` doesn't exist, so there is nothing
  for a test to compare the checkpoint set against. TODO gate, once `tests/e2e/` and the
  checkpoint-file scaffolding land: a test asserting the checkpoint set and the e2e file set
  match (see the plan's Wave 1 step 10).
- Property tests (hypothesis) and contract tests (schemathesis) are planned but not yet present
  in this tree; cite `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` Wave 2 for what lands
  and why.
- e2e tests are driven via the `playwright-cli` skill for manual QA, never raw
  `@playwright/test`, per the repo owner's global tooling policy.
