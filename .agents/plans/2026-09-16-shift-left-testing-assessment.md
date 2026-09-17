# Shift-left testing assessment (2026-09-16)

Read-only adversarial review from 4 POVs (type-system, FSM/ADT, property/contract,
architecture/static-analysis), synthesized against the prior
`2026-09-16-testability-assessment.md` and `2026-09-16-agentic-first-roi-top-5.md`.

**Verdict in one line**: shift-left helps **narrowly and cheaply** (Clock injection,
telematics-as-test, a structured CI tier, and 2 real bug fixes), but most
proposed techniques (full mypy, hypothesis-by-default, schemathesis, mutation
testing, third-party scanners) are net-negative ROI for a 22 MB process that
already uses pydantic + Protocol + dataclasses correctly.

## Top 4 adopt-now moves

1. **`Clock` injection in `oxe/cache.py` and `oxe/search.py`.** Single highest
   leverage. Destroys the brittle backdating hack at
   `tests/test_api_history.py:93-97` (`UPDATE clicks SET clicked_at =
   clicked_at - 90000` from outside the lock). Also makes eventual SVG snapshot
   tests deterministic. (FSM/ADT POV's #9, agrees with property POV ranking.)
2. **Wire up a real `pytest` step in CI**. Today `.github/workflows/ci.yml:9-44`
   only does `pip install` + boot + `curl /health`. Zero tests run on PRs. That's
   the most expensive shift-left gap in the whole review.
3. **Fix the two confirmed bugs** (see below). They are cheap (~5 LOC each) and
   invisible today only because we have no MCP test and an
   enshrining-the-bug unit test.
4. **Replace `make_state()` factory at `oxe/server/state.py:41`** *instead of*
   only a `tests/conftest.py`. Kills the import-time fd leak in prod, dev, and
   tests for ~5 LOC. A `conftest.py` that sets `OXE_CACHE_DIR` is the wrong fix
   — it leaves the bug in prod and codifies it as a "test concern."

## CI tiering (architecture POV)

Today's `.github/workflows/ci.yml` is smoke-only — there is no `pytest`,
no `ruff`, no `bun test`, no `tsc` gate. This is priority zero.

| Tier     | Trigger                  | What runs                                                                                             | Runtime   | Notes                                                                                                                       |
|----------|--------------------------|--------------------------------------------------------------------------------------------------------|-----------|------------------------------------------------------------------------------------------------------------------------------|
| **T1**   | every push to PR         | `ruff check oxe tests` + `pytest -q tests/test_mcp_server.py tests/test_api_history.py` (fast subset) | ~15-20s   | Same bones as today's `mise run validate`, minus slow tests; matches the budget of `[githooks]validate` already in `mise.toml` |
| **T2**   | PR merge to main         | full `pytest` + `ruff check` + UI `bun test src` + `oxlint` + `tsc -b`                                | ~60-90s   | Catch-all gate. Today nothing in the matrix hits this.                                                                       |
| **T3**   | nightly cron             | + `pip-audit` (audit-only) + `bandit -q oxe/` (suppress expected) + OpenAPI status-walk + size-budget | ~5-10 min | Failure creates an issue, does not block merges.                                                                            |

Cost: GitHub-hosted ubuntu-latest is $0.004/min. T1 at 30 PRs/day × 20s = 10
min/day. T2 at 10 merges/day × 90s = 15 min/day. T3 nightly = 10 min/day.
**Total ≈ $0.30/mo.** No reason not to run all three.

## Confirmed bugs (all 4 POVs agree)

### Bug 1 — `oxe/mcp_server.py:62-64` clamp gap (worse than first flagged)

```python
if source == "history":
    rows = _cache.get_clicks(query_text=query, limit=num_results) if _cache else []
```

Two compounding problems:

- `num_results` (advertised as "1-30, default 10" in the tool description at
  `oxe/mcp_server.py:33-37`) flows straight into `cache.get_clicks(limit=…)`
  with no clamp at the MCP boundary. `exa_user_history` does clamp
  (`oxe/mcp_server.py:102-103`); `exa_search` does not.
- `since_hours` is **not passed at all**. `cache.get_clicks` defaults
  `since_hours=None` → `since=0` → returns **every click ever recorded**. So
  the history branch is both un-clamped and unbounded in time.

**Fix** (~3 lines, paste above the `if source == "history":` branch):

```python
lim = max(1, min(int(num_results), 30))
since = 168 if since_hours is None else max(1, min(int(since_hours), 24 * 30))
```

(Yes, the tool description field needs `since_hours` added; it isn't there
today. JSON Schema validation per property-POV technique #9 would have caught
the description vs signature drift on day 1.)

The same `num_results` clamp applies in the default branch and is already
implicit in `exa_compat.search` via `_NUM_CLAMP = (1, 30)` at
`oxe/exa_compat.py:103`. Adding it at the MCP layer makes the contract
self-evident.

### Bug 2 — `oxe/ai.py:198-210` `parse_final_answer` eats confidence when JSON lacks `related_questions`

The parser loop:

```python
for i in range(len(lines) - 1, max(len(lines) - 4, -1), -1):
    line = lines[i].strip()
    if line.startswith("{") and line.endswith("}"):
        m = CONFIDENCE_RE.search(line)
        if m and '"related_questions"' in line:
            ...
            body = lines[:i]
            break
```

The gate `'"related_questions"' in line` means a perfectly valid JSON like
`{"confidence": 9}` is treated as body text and confidence is silently dropped
to 0. The "test" at `tests/test_ai_pipeline.py:91-93` (`test_parse_final_answer_garbled_confidence_only`)
actually verifies `conf == 0` — and the test name is misleading: it asserts the
`related_questions` guard, not confidence parsing in absence of related questions.

**Repro**:

```python
from oxe.ai import parse_final_answer
body, conf, _ = parse_final_answer("Here is the answer.\n{\"confidence\": 9}")
assert body == "Here is the answer.\n{\"confidence\": 9}"
assert conf == 0   # BUG: should be 9
```

**Fix** (~6 lines): split the gate into two, and accept confidence-only JSON
as valid:

```python
if line.startswith("{") and line.endswith("}"):
    m = CONFIDENCE_RE.search(line)
    if m and ('"related_questions"' in line or '"confidence"' in line):
        try:
            parsed = json.loads(line)
        except (json.JSONDecodeError, ValueError):
            parsed = {}
        try:
            conf = int(parsed.get("confidence") or 0)
        except (TypeError, ValueError):
            conf = int(m.group(1)) if m else 0
        if '"related_questions"' in line:
            related = parsed.get("related_questions") or related
        body = lines[:i]
        break
```

Then **flip the test** at `tests/test_ai_pipeline.py:91-93` to assert
`conf == 7` — the model intends the confidence to be saved, not lost.

### Bug 3 — `oxe/ai.py:207-208` uncaught `AttributeError` on `int(re_match.group(1))`

Partially surfaced by the property POV. The regex `'"confidence"\s*:\s*(\d+)'`
against `{"confidence": "abc"}` doesn't match at all (`m is None`), so even the
fallback `conf = int(m.group(1))` at `oxe/ai.py:208` raises `AttributeError`,
which is **not** in the `(json.JSONDecodeError, ValueError)` catch. The same
fix as Bug 2 handles this.

### Bug 4 — `oxe/cache.py:32-34` swallows all `OperationalError` on ALTER

```python
try:
    self._conn.execute(f"ALTER TABLE {table} ADD COLUMN {col} {decl}")
except sqlite3.OperationalError:
    pass
```

If the schema file drifts or the column spec is malformed, the `ALTER` raises
something other than "duplicate column" and the swallow hides it. A follow-up
query then fails with a less helpful error (and confuses the schema-assertion
technique in architecture-POV #2).

**Fix** (≈2 lines): narrow the catch.

```python
except sqlite3.OperationalError as e:
    if "duplicate column" not in str(e).lower():
        raise
```

## Net-positive adopt (additional)

| Adopt                              | Why                                                                                                  | Source                         | Effort |
|------------------------------------|------------------------------------------------------------------------------------------------------|--------------------------------|--------|
| `Clock` injection in `TTLCache`    | Highest single leverage. Enables timestamp-deterministic tests without the private-`_lock` SQL hack. | FSM/ADT #9 + property POV      | S-M    |
| Wire `pytest` to CI (T2)           | Today's CI is smoke-only.                                                                              | Architecture POV (#4 / CI tiering) | M      |
| `make_state()` factory             | Kills import-time fd leak for everyone, not just tests; supersedes the conftest-only fix.            | Architecture POV #9            | S      |
| Telemetry-as-test (`OXE_DEV=1`)    | Asserts the `event=search` JSON shape on stdout after `/search`. `oxe/devlog.py:23` already exists.   | Architecture POV #6            | S      |
| `tests/test_mcp_server.py` (added) | 80-line file, 0 tests today. Correctly the biggest single test gap.                                    | Testability assessment #2, architecture POV | S      |
| `tests/conftest.py` w/ session tmp cache | Belt-and-braces even after `make_state()`; pairs with the factory refactor.                       | Testability assessment #1 (also architecture POV #1) | S      |
| Schema assertions on `oxe/sql/*.sql` | Catches drift between SQL files and runtime queries; ~30 LOC.                                        | Architecture POV #2            | S      |
| OpenAPI per-endpoint status walk   | Catches route regressions; `openapi.json` already exists.                                            | Architecture POV #11           | S      |
| Hypothesis for `parse_final_answer` | Covers Bug 2 + Bug 3 going forward; this is the one place hypothesis earns its keep.                  | Property POV #3                | S      |
| Hypothesis for `cache_key` (60 LOC) | Catches silent normalization regressions that cause cross-query leakage.                             | Property POV #1                | S      |
| JSON Schema validation at MCP tool boundary | Cheaper than schemathesis, ~30 LOC, catches Bug 1 retroactively.                                  | Property POV #9                | S      |
| Selective `ruff` expansion (PERF + RUF subset) | Skips G004 (logging-f-str); oxe already uses %-format.                                            | Architecture POV #5            | S      |

## Net-negative adopt (reject)

- **Full `mypy --strict` on the whole package.** Type-system POV ran mypy and
  found 41 errors in 9 files. Architecture POV correctly observes that
  `oxe/AGENTS.md:8` forbids adding type checkers without owner signoff.
  Property POV adds that cache + exa_compat — the only files type-clean today
  — are also the lowest-value targets.
  **Decision: skip unless owner asks.** The owner rule exists for a reason;
  this report does not override it.
- **`mypy --strict` on cache + exa_compat only.** Clean today, low value, low
  friction. Doesn't catch the real bugs (in `ai.py`, `mcp_server.py`).
- **Pydantic on internal function boundaries.** 30× slower than duck-typed
  `dict.get`; `cache_key` runs on every search; the pydantic-on-HTTP /
  dataclass-on-internal split is the right tradeoff.
- **NewType / branded types** (`query_hash`, `cache_key`, `url`, `RowId`).
  Purely static; doesn't survive dict or SQLite Row access without unrealistic
  ceremony.
- **TypedDict for MCP tool return shapes.** Pydantic schemas at
  `oxe/server/schemas.py:63-242` are already the source of truth.
- **`@typing.override` on Protocol impls.** Two concrete impls per Protocol +
  duck-typed `Fake` in tests means `@override` doesn't fire where it matters.
- **TypeGuard / TypeIs refiners.** Mypy already narrows from `if hit is not
  None`. Theater.
- **Stubs for `ddgs`.** Drift risk; current `patch.object(exa_compat, "DDGS",
  FakeDDGS)` pattern is the right abstraction. (Type POV #5)
- **`Result` / `Either` return types in `do_search` and `TTLCache.get`.** Python
  raises by convention; codebase already does errors-as-values at the
  right layer (`BackendError`).
- **`Optional[T]` → `Missing`/`Present` with attrs.** Pure style tax.
- **XState / statecharts for the UI or MCP dispatcher.** `useAnswer.ts:31-54`
  already uses the discriminated-union + reducer pattern. Two MCP tools = a
  switch, not an FSM.
- **Branded `BackendName`.** Current free-form `str` + entry_points registry is
  right for third-party plugins.
- **`hypothesis` blanket policy.** Only `parse_final_answer` and `cache_key`
  earn it (Bug 2 + Bug 3 are invariant failures). Already-pinned targets
  (`answer_cache_key`, `do_search` backend shape) won't be helped.
- **`mutmut` / `cosmic-ray` mutation testing at this scale.** 30+ min/AST on
  3K LOC for ~5% extra coverage. Use it postmortem on a real bug, not as
  blanket safety net.
- **`pytest-approvaltests` whole-response JSON snapshots.** Existing goldens
  in `test_api_stats.py` and `test_openapi_fresh.py` give reviewer-readable
  diffs without the dep.
- **`syrupy` inline HTML snapshots of `GET /search`.** Only HTML oxe serves is
  the inline notice at `oxe/ui_dist.py`; already covered by
  `tests/test_ui_dist.py`. Snapshotting the SPA bundle = testing Vite, not
  oxe.
- **Structural SVG snapshots as a standalone PR.** Pair with the
  `tests/test_stats.py` PR (already recommended in the testability assessment)
  for combined value; otherwise substring asserts (already covered
  indirectly) are enough.
- **`pip-audit` / `bandit` / `safety` as blocking CI gates.** 6-dep tree;
  bandit flags legitimate `try: ... except OperationalError: pass` and the
  intentional `except Exception` blocks in `mcp_server.py:80` and `ai.py`.
  Run as **nightly audit reports** (T3 audit-only), not as merge blockers.
- **`OXE_TEST=1` env var + global fake `DDGS()`.** The current
  `monkeypatch.setattr(exa_compat, "DDGS", FakeDDGS)` pattern at
  `tests/test_backends.py:15-30` is more transparent per-test.
- **SQL `CHECK` constraints on TTL>0 / `expires_at > created_at`.** Schema
  today uses `CREATE TABLE IF NOT EXISTS` + on-the-fly `ALTER TABLE ADD
  COLUMN`. Adding CHECK constraints requires either a migration step or living
  with the lax schema; not worth it for the marginal protection.
- **Inline `assert payload["_source"] in {"cache","network"}`** at the HTTP
  boundary. Replace with a single pytest in
  `tests/test_x_cache_header_agrees_with_source`.

## The shift-left trap (consensus)

What type/FSM/property tests fundamentally **cannot** catch:

- Real `ddgs` library version drift (the project's `#1` source of silent
  behavioral change since v0.3).
- DDG rate-limiting and captcha behavior under load.
- OpenAI/Anthropic rate-limit and partial-stream semantics.
- SQLite WAL contention under concurrent `OXE_CLICK_RETENTION_DAYS` GC.
- DNS rebinding against `127.0.0.1`-only binding.
- HTTP/2 vs HTTP/1.1 keep-alive edge cases in `httpx`.

**Bias toward observability + a small amount of shift-left**, not more tests.
The `mise run logs` task + `OXE_DEV=1` telemetry is the highest-leverage
observability tool already built; it needs CI coverage to be trustworthy.

## PR sequencing (suggested; owner decides)

Smallest blast-radius first. Each PR is independently revertable.

1. **PR-S1** (≈40 LOC) — `make_state()` factory + `tests/conftest.py` + the
   two `mcp_server.py` bugs (Bug 1 only — clamp + `since_hours`).
2. **PR-S2** (≈30 LOC) — `Clock` injection in `TTLCache` + `do_search`; flip
   the misleading `tests/test_ai_pipeline.py:91-93` to assert `conf == 7`;
   apply Bug 2 + Bug 3 fixes in the same PR.
3. **PR-S3** (≈80 LOC) — `tests/test_mcp_server.py` covering both tools + JSON
   Schema validation at the MCP boundary (catches Bug 1 retroactively).
4. **PR-S4** (≈30 LOC) — telemetry-as-test (`assert stdout["event"] ==
   "search"` after `POST /search` under `OXE_DEV=1`).
5. **PR-S5** (≈60 LOC) — hypothesis on `parse_final_answer` + `cache_key`.
6. **PR-S6** (≈30 LOC) — OpenAPI per-endpoint status walk; SQL schema
   assertions; selective `ruff` PERF + RUF.
7. **PR-S7** (≈45 LOC) — CI tiers T1/T2/T3 wired in `.github/workflows/ci.yml`.

PR-S1, PR-S2, PR-S3 unblock one another. PR-S4 needs PR-S1 (factory) so the
`OXE_DEV` test reads the right cache. PR-S5 needs PR-S2 (Clock). PR-S7 is
the integrated gate and goes last.

## Disagreements among the POVs

- **Type POV vs Architecture POV on mypy**: Architecture POV defers to
  `oxe/AGENTS.md:8`'s "no type checkers without owner asks" rule. Type POV
  found 41 latent errors. **Resolution**: do not add mypy; the AGENTS.md rule
  exists and the owner can override on demand. If the owner wants it, the
  `SearchBackend | None` re-typing is the single change that turns mypy from
  noise to signal.
- **FSM/ADT POV vs Property POV on which bug at `ai.py:209` is real**: FSM/ADT
  reads `ai.py:209` as a body-stripping bug; Property POV walks the control
  flow and the property POV's read is correct — the bug is the silent
  confidence drop, not the body preservation. The same fix in `ai.py:198-210`
  covers both readings.
- **Testability assessment vs Architecture POV on conftest**: Testability
  assessment ranks "no conftest.py" as the #1 debt. Architecture POV argues
  `make_state()` is better because it fixes prod too. **Resolution**: do both.
  Factory first; conftest is the belt-and-braces fallback.
- **All POVs vs the owner rule on linters**: `oxe/AGENTS.md:8` is clear.
  Mypy/bandit/safety are flagged as low-ROI but the decision is the owner's.

## Files referenced

- `.agents/plans/2026-09-16-testability-assessment.md` (prior)
- `.agents/plans/2026-09-16-agentic-first-roi-top-5.md` (prior)
- `oxe/AGENTS.md:1-50` (lint/typecheck policy)
- `oxe/cache.py:1-340` (TTLCache, get_clicks, ALTER swallow)
- `oxe/cache.py:32-34` (Bug 4 location)
- `oxe/ai.py:180-211` (parse_final_answer — Bug 2, Bug 3)
- `oxe/mcp_server.py:1-105` (Bug 1 — clamp gap)
- `oxe/mcp_server.py:62-64` (history branch)
- `oxe/mcp_server.py:102-103` (the existing clamp in `exa_user_history`)
- `oxe/server/state.py:1-42` (module-level cache singleton)
- `oxe/exa_compat.py:103` (`_NUM_CLAMP = (1, 30)`)
- `oxe/devlog.py:1-35` (telemetry surface)
- `oxe/sql/{schema,cache,clicks,answers,search_log}.sql` (no CHECK constraints)
- `tests/test_ai_pipeline.py:90-93` (the enshrining-the-bug test)
- `tests/test_api_history.py:92-96` (backdating hack Clock-injection would
  remove)
- `tests/test_backends.py:15-30` (Fake backend pattern)
- `tests/test_openapi_fresh.py` (already-thin OpenAPI test to expand)
- `.github/workflows/ci.yml:1-44` (smoke-only CI)
- `mise.toml:1-133` (validate + lint + test task shapes)
