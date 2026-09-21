# Commit the wave 3/4 work, then close cheap UI loop findings

Context: UI loop run 1 ended FAIL at the 3-iteration cap (`.agents/notes/2026-09-18-ui-loop-run1.md`).
Work is uncommitted. GitHub Project linkage was explicitly skipped by the user.
Constraints: no Co-Authored-By trailers, atomic conventional commits, no push, no hook skipping.

## Phase A: commit the working tree (one subagent, sequential, git index is shared)

Proposed groups, in order. The subagent may split further but must not merge across groups.
Where one file mixes two groups (for example `ui/src/routes/search.tsx`, `ui/src/components/SearchBox.tsx`),
commit it in the group it primarily belongs to; do not use `git add -p` gymnastics.

1. `feat(search): add vendored Wikipedia OpenSearch test engine`: `oxe/search/engines/wikipedia.py`,
   `oxe/search/__init__.py`, `oxe/search/engines/registry.py`, `tests/search/test_wikipedia_engine.py`,
   `tests/search/test_registry.py`, `.agents/plans/2026-09-18-wikipedia-opensearch-test-backend.md` if tracked-eligible.
2. `test(api): stream a partial delta before the force=error answer fixture`: `oxe/api/answer.py`, `tests/api/test_answer.py`.
3. `docs(screens): add UI loop rubric, reference patterns and insights dispositions`: `.agents/docs/screens/rubric.md`,
   `.agents/docs/screens/references/`, the four screen specs (`landing|search|history|dashboard.md`).
4. `feat(ui): shared search box, results shell and loader-as-state`: `ui/src/components/{SearchBox,Shell}.tsx`,
   `ui/src/lib/searchLoader*.ts`, `searchReducer*.ts`, `ui/src/routes/{index,search,history,dashboard,__root}.tsx`,
   `ui/src/index.css`, `ui/src/locales/en/messages.po`, `ui/src/lib/effects/answerStream.ts` (split the stream fix into
   its own `fix(ui): key the answer stream effect and abort on cleanup` commit if the file set allows it cleanly).
5. `chore(ui): add axe-core, pin swc verifier message, use temp cache in e2e config`: `ui/package.json`, `ui/bun.lock`,
   `ui/scripts/verify-swc-pipeline.ts`, `ui/playwright.config.ts`, `ui/AGENTS.md`.
6. `test(e2e): add checkpoint baselines and tighten checkpoint 3`: `tests/e2e/checkpoints.md.ts`,
   `tests/e2e/checkpoints.md.ts-snapshots/`, `tests/test_e2e_adapter_parity.py`.
7. `feat(tooling): add UI loop evidence capture script`: `scripts/ui-loop-capture.js`.
8. `docs(agents): add v0.5.0 plan progress and UI loop run 1 ledger`: `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`,
   `.agents/notes/2026-09-18-ui-loop-run1.md`, this plan. Skip any path that `.gitignore` excludes; report it, do not force-add.

Gate before each commit: `git diff --cached --stat` matches the group; no `.env`, no `test-results/`, no temp dirs.
Gate at the end: `git status --short` empty except ignored files; `git log --format=%B` contains no `Co-Authored-By`.

## Phase B: cheap UI findings (parallel implementers, file-disjoint, code only)

Each implementer edits only its files, runs `bunx tsc --noEmit`, `bun run lint`, `bun run test` in `ui/`,
does NOT start servers, does NOT touch e2e baselines, and does NOT commit (orchestrator commits per fix).

- B1 title underline: `ui/src/routes/search.tsx` result title link and AI source cards: no resting underline, underline on hover and focus-visible.
- B2 hero anchor: `ui/src/components/Shell.tsx` hero padding so the pill centre lands at 42-45% of viewport at 1280x720, 768x1024, 390x844. Measure by formula, verify in Phase C.
- B3 header chrome: `ui/src/routes/__root.tsx` active nav link marked (brackets per `landing.md` Behavior "Header nav") and the version label at >=640px; version source must be the real package/app version, not a literal invented in the component.
- B4 dashboard cache panels: `GET /api/stats` ships no `cache` key today (found while planning), so this is backend plus UI. Add a `cache` object (rows, unexpired, db size, newest) to the stats summary in `oxe/` with pytest coverage, regenerate `ui/src/lib/types.gen.ts` via the repo's own generator, and bind the hit-rate and cache panels in `ui/src/routes/dashboard.tsx` per `dashboard.md`. Placeholders stay only for log-derived panels. Owns: `oxe/**` stats code, `tests/**` for it, `ui/src/lib/types.gen.ts`, `ui/src/lib/historyApi.ts`, `dashboard.tsx`.

Explicitly queued, not attempted: result snippet and cached preview (backend data), cache age (backend), zero/empty-source glyph and confidence meta (needs a checkpoint 14 decision).

## Phase C: verification and evaluation (orchestrator plus one isolated evaluator)

1. Commit B1..B4 as four `fix(ui)` / `feat(ui)` commits.
2. Restart test backend (`OXE_BACKENDS=wikipedia-opensearch`, temp `OXE_CACHE_DIR`, port 4577), portless vite, `playwright-cli open`; run `scripts/ui-loop-capture.js`; hard gates must be 27/27 green.
3. e2e: kill my backend first (port 4577 conflict), `--update-snapshots`, run twice, review changed baselines visually, commit as `test(e2e): refresh baselines after loop fixes`.
4. One fresh isolated evaluator (opus, no diff, no rationale, no prior verdict, no insights/). Budget 3 iterations for this batch, ledger continues in a new note `.agents/notes/2026-09-21-ui-loop-run2.md`. A finding that reappears after being marked fixed terminates the run.
5. Write-back on PASS: update the screen specs and baselines in the same commit as the code, per UILOOP section 8.
6. Full gates: `uv run pytest`, `bun run test`, tsc, lint, `diff -q` of the two plan copies, em dash scan of new docs.
7. Cleanup: kill only my processes (never the user's 8899 server, never oxmgr).

## Reporting

Final report lists commits, evaluator verdict, and what remains queued.
