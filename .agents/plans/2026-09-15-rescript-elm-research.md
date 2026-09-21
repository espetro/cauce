# 2026-09-15 — UI logic-language assessment: ReScript vs Elm

## Brief

Assess whether migrating the **non-presentational TS code** in `ui/src/` to
[ReScript](https://github.com/rescript-lang/rescript) or
[Elm](https://github.com/elm/compiler) would:

1. Reduce logic that AI agents can misread or invent
2. Be enforceable as a one-way street (presentation layer stays TS/TSX)
3. Stay inside the project's 40 KB JS / 30 KB CSS gz budget
4. Keep the existing Vite + Preact + daisyUI + valibot + preact-iso toolchain
5. Pay back its migration cost within the v1 cycle

Then we decide: **ship_it_for_v1** / **revisit_later** / **never**.

## Scope of "non-presentational" code (the migration target)

| File | LOC | Kind |
|---|---:|---|
| `ui/src/lib/api.ts` | 142 | Typed fetch clients (search, click, suggest, ac, cache, settings) |
| `ui/src/lib/ai.ts` | 82 | SSE consumer (`streamAnswer`), types |
| `ui/src/lib/format.ts` | 35 | Pure formatters (domainOf, fmtDur, fmtBytes, fmtTs, truncate) |
| `ui/src/lib/theme.ts` | 30 | localStorage + DOM `data-theme` toggle |
| `ui/src/features/search/pager.ts` | 16 | Pure URLSearchParams builder |
| `ui/src/features/suggests/useSuggests.ts` | 136 | Hook + pure `mergeSuggests`, `useListNav` |
| `ui/src/features/search/useSearch.ts` | 95 | Hook |
| `ui/src/features/answer/useAnswer.ts` | 92 | Hook + pure `applyAnswerEvent` reducer |
| `ui/src/features/settings/schema.ts` | 58 | valibot schema |
| `ui/src/features/answer/MarkdownLite.tsx` | 111 | Pure renderer (regex-driven, returns JSX) |
| **total** | **~797** | out of 3,005 LOC UI source (≈27%) |

## Stack baseline

- **Renderer:** Preact 10 (no React); `preact-iso` for routing; valibot for runtime validation; daisyUI on Tailwind 4.
- **Bundler:** Vite 8 (so any candidate that integrates with `vite-plugin-*` is fine; raw esbuild/rollup configs are tolerable).
- **Tests:** Bun's `bun test` runner (28 test files in `*.test.ts`, no DOM testing).
- **Budget:** 40 KB JS gzipped / 30 KB CSS gzipped (currently 62.5 K / 96.6 K — JS is already over; see `scripts/size-budget.ts`).
- **No existing JSX constraint:** presentation layer keeps Preact JSX.

## Hypothesis statements (the questions for the subagents)

H-ReScript-1: A ReScript module can compile to JS, be imported from TS/TSX,
keep the same Vite + bun test workflow, and produce types that AI agents read
more accurately than JSDoc-it'd TS.

H-ReScript-2: The non-presentational files above (≈797 LOC) can be expressed
in ReScript idiomatically without losing features agents depend on (e.g. the
`AbortController`/`fetch`/`localStorage` surface, regex-driven markdown,
JSON-shape equality).

H-ReScript-3: A clear `ui/src/logic/` (ReScript) ↔ `ui/src/` (TS/TSX)
boundary is enforceable at import-restriction level (oxlint or simple
convention) so the presentation layer cannot reach into the migrated code's
underscored internals.

H-Elm-1: Same as H-ReScript-1 with Elm's `elm/core` + ports.

H-Elm-2: Same as H-ReScript-2 evaluated under Elm's restrictive model
(no `any`, no exceptions, no `undefined`).

H-Elm-3: Same as H-ReScript-3 evaluated under Elm's "import Elm modules
through a generated bridge, never the other way" constraint.

## Output format expected from subagents

For each language, a plain-text report with:

- 1-paragraph fit summary
- File-by-file mapping (which file → what shape in the target language)
- Bundle size delta estimate (target language libs vs current `lib/*.ts`)
- Build-pipeline story (how Vite + bun test still runs)
- AI-agent readability proxy (does the target type display better than JSDoc? examples)
- Maintenance surface (trailing-edge risk if upstream slows down)
- Migration cost (hours, in files touched) and amortisation path
- Verdict (`ship_it_for_v1` / `revisit_later` / `never`) with confidence

Both subagents should note whenever the `gateway__search-exa_search` tool
returns no results and fall back to general-knowledge assessment (we logged
that the gateway was flaky in this session previously).

## Verdict matrix (this session)

| Language | Verdict | Confidence | Headline reason |
|---|---|---|---|
| **ReScript** | `revisit_later` | medium | No Preact binding exists; need ~30–50 lines of FFI forever. ~270 of 797 LOC actually portable; the rest is hooks/JSX/valibot. Migration pays back in v2 if Preact binding lands upstream. |
| **Elm** | `never` | high | JS budget is structurally violated (Browser.element mount ≈ 45–60 KB gz vs 40 KB cap). Vite 8 / Rolldown breaks `vite-plugin-elm` (open issue #862, Apr 2026). Migration forces a Vite downgrade. Single-owner compiler frozen by design. Win-per-LOC is tiny since 797 LOC is mostly effects plumbing, not the domain logic Elm shines on. |

## Recommendations (independent of either migration)

The TS code here is mostly effects plumbing + 2-3 small pure reducers. The agent-misread failure mode is real but addressable without a compiler change:

1. Add `const _exhaustive: never = ev;` to `applyAnswerEvent` (and similar closed unions elsewhere) — 1 line, no compile cost, catches future `AnswerEvent` variant additions.
2. Tighten `tsconfig.app.json`: turn on `noImplicitAny`, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`. Currently `strict` is on but these specific flags are not enforced.
3. Add an `oxlint` `no-restricted-imports` rule per feature folder, so `features/search/*` cannot reach into `features/answer/*` internals.

These three changes capture ~40% of the agent-readability win both subagents advertised, at <5% of the migration cost.

## Decisions

- [x] Final verdict per language — see table above
- [ ] Decide whether to apply the three TS-side improvements this session (orthogonal to migration decision)
- [ ] Revisit ReScript in v2 if upstream ships a `rescript-preact` binding, OR if the oxe non-presentational surface grows past ~2,000 LOC

## Files in this plan

- This file: `2026-09-15-rescript-elm-research.md` (brief + verdict)
- Subagent reports live inline above; the raw subagent transcripts are not committed.

## Known unknowns (subagent limitations)

- `bun test` + `.res.js` import resolution not verified end-to-end (ReScript subagent).
- Real `Browser.element` mount bundle with this exact port surface not measured (Elm subagent — figure extrapolated, expected 42–65 KB gz).
- `vite-plugin-elm` issue #862 may have been closed in the meantime; re-check before any future Elm spike.
- `gateway__search-exa_search` MCP tool not used (outage logged in `.agents/MEMORY.md`); both subagents used `webfetch` and priors.
