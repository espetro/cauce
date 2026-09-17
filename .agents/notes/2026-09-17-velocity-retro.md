# v0.4.0 velocity retro (carried forward from the archive-rebuild plan)

Provenance note: this note reconstructs the retrospective from the figures already recorded in
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` (Context section, and the "Python: the
same treatment" / "The API contract is typed in both directions" measured-on-`legacy` tables).
`legacy` (the pre-reset `main`, `feat/v0.4.0-web-ui`) has no `.agents/notes/` directory at all —
checked via `git ls-tree -r legacy --name-only | grep agents` before writing this note — so
there is no separate retro file to carry forward verbatim; the plan's Context section *is* the
retro write-up. This note exists so a future session doesn't have to re-derive the numbers from
inside the plan doc.

## Headline finding

The cost on `feat/v0.4.0-web-ui` (54 commits) was not in the features. It was in five
cross-cutting policies — daisyUI, no-`useEffect`, generated types, i18n, typed routes — applied
*after* the screens already existed:

- 11,565 lines of churn, ~30% of total effort, retrofitting policies that were never gates.
- Zero CI runs and no git hooks on that branch the whole time — no automated gate ever fired.
- 45% of the diff was machine-generated (codegen, reformatting), not hand-written feature work.
- 33% rework rate.

## Backend-specific findings (measured on `legacy`)

- `model_config = {"extra": "allow"}` on all 18 wire models in `server/schemas.py` — the
  mechanism behind the `PUT /settings` bug that silently wiped `base_url` for 10 hours.
- `uv.lock` gitignored (twice, at two different lines of `.gitignore`) — no reproducible
  install; local and CI resolved independently.
- 61 bare `dict` signatures, 49 broad `except` clauses (including
  `except sqlite3.OperationalError: pass` in the cache migration), 14 `Any`, 21 ruff ignore
  entries including `BLE001` and four `TRY` rules — the linter was configured to not report the
  defects above.
- Zero type checkers configured; a stale `.mypy_cache` was the only trace anyone had run one.
- No Python formatter, hence a 1,461-line reformat commit landing 32 seconds after the commit
  that should have carried it.

## API contract findings

- Of 22 operations in `legacy`'s `openapi.json`: 11 had no typed 200 response schema, 3 mutating
  operations had no request body schema, and only 11 of 22 declared `response_model` at all.
  `GET /suggest` published as `array of {}`; `GET /search`, `GET /history`, `POST /answer` and
  `POST /history/delete` published nothing. Generated TS types described roughly half the
  contract — and the half they described was the half least likely to break.
- `POST /answer`'s SSE frames were entirely outside OpenAPI's reach, and that is exactly where
  two real parse defects were found.

## Architecture bugs (from `.agents/MEMORY.md` on `legacy`, 2026-09-17 entry)

- `delete_clicks` only deleted the `clicks` table, not `search_log` — dashboard stayed stale
  after "delete all".
- `7d`/`30d` scopes silently returned 0 (`oxe/cache.py:331-340` on `legacy`).
- History rows linked to `/search?q=...` without `mode` — mode was lost on click-through.
- `search_log` had a `source` column but no `mode` column.
- Dashboard read `search_log`, History read `clicks` — two unsynced data planes for what should
  have been one.

## What this rebuild changes structurally

Every one of the above becomes a day-0 gate instead of a retrofit: `extra="forbid"` on wire
models, `uv.lock` committed, ruff `select` expanded with an empty `ignore`, basedpyright strict,
`response_model` required on every route with `tests/test_openapi_coverage.py` enforcing it, SSE
frames typed as pydantic models registered as OpenAPI components. See
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` for the full gate list and wave plan, and
`.agents/decisions.md` for the locked decisions this retro fed into.
