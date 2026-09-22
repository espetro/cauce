# Verdict dispositions

Every `## Verdict for cauce` item from the five product docs, with exactly one
disposition. Nothing is adopted by drift. Dispositions:

- **covered**: already true in a pattern doc or spec; no change.
- **promoted**: written into a pattern doc (`../patterns-*.md`).
- **queued**: written into the owning screen spec under `## Queued improvements`.
  Not built, so the evaluator does not check it until it moves into the spec body.
- **dropped**: rejected, with the reason.

The evaluator never reads this file; it only sees what these dispositions
promote into the pattern docs and specs.

| # | Item | Disposition | Where / why |
|---|------|-------------|-------------|
| G1 | Persistent in-composer AI chip | covered | `patterns-search-input.md` |
| G2 | Favicon citation chip with overflow counter | queued | `search.md`. Google plus Brave's citation events both carry favicons, but DuckDuckGo's chips are the weakest treatment seen, so it is a spec decision, not a converged rule. Fallback stays `[n]`. |
| G3 | Keep horizontal source-card row, add Show all | queued | `search.md`. Row is already the spec; only the expansion is new. |
| G4 | Skip shimmer-only thinking | covered | `patterns-states.md` (progressing status line) |
| G5 | Low-confidence fallback to classic results | covered | `patterns-states.md` (AI mode unavailable) |
| D1 | Keep per-claim `[n]` above DDG's weak citations | covered | `patterns-answer-streaming.md` `## Citations` (inline, at the claim) |
| D2 | Stale cache on throttle state | promoted | `patterns-states.md` as a cauce-specific state. The service treats expired rows as misses today, so the behavior is queued in `search.md`. |
| D3 | Privacy chrome: "served locally, cached on your machine" | queued | `history.md` and `search.md`. One product; copy decision only. |
| D4 | Motion floor: no animation on SERP render | covered | `patterns-motion.md` lists only streaming, phase and expand/collapse motion, plus reduced-motion; no SERP entrance motion exists to remove |
| D5 | Active model shown inline, capability annotations in the dropdown | queued | `landing.md` already has the in-pill model picker; only the annotations are new and need backend model metadata that `/v1/models` does not expose. |
| B1 | Two-tier AI split, cheap always-visible hatch | covered | `patterns-hybrid-serp.md` |
| B2 | Decoupled loading of the two modes | covered | Built: `ui/src/lib/searchLoader.ts`, pinned by `searchLoader.test.ts` |
| B3 | Progressive inline citations during the stream | covered | `patterns-answer-streaming.md` |
| B4 | Keep explicit empty-sources and mid-stream states | covered | `patterns-states.md` |
| B5 | Goggles-style local, undoable transparency panel | queued | `search.md`, cache meta line. Brave only; interaction grammar, not a rule. |
| M1 | Do not adopt the answer-only frame | covered | `patterns-hybrid-serp.md`, `patterns-layout-grid.md` |
| M2 | Composer morph landing to results | queued | `landing.md`. Pill must land top-of-column, per the recorded bottom-pin divergence. |
| M3 | Collapsible streamed Thoughts region | dropped | Backend emits no reasoning or phase events; the skeleton plus status line stays. Reopen if the SSE frame union gains a phase frame. |
| M4 | Grounding chips showing queries actually run | queued | `search.md`. Not covered: the spec has related questions, not grounding chips, and the backend does not currently expose the queries it ran. |
| M5 | Keep gradient branding and Google Sans at arm's length | covered | `patterns-typography.md` (system font stack, Google/Gemini divergence noted) |
| S1 | Cache-first render, refetch only on `cached` click | covered | `search.md` transparency meta line with clickable `cached` badge |
| S2 | Failure surface naming the failed source, silent on success | promoted | `patterns-states.md`. With one upstream the "source" is the backend, so the copy names the engine and error class. |
| S3 | Distinct page-1 versus page-N empty copy | promoted | `patterns-states.md`; cauce uses continuous scroll, so page-N is "end of results". |
| S4 | Collapsed answer box on load, citation-click behavior required | covered | `patterns-answer-streaming.md` (reserved space, collapsed full source list, citation aria-labels) |
| S5 | Transparency budget goes to cache freshness affordances | queued | Same item as B5; one entry in `search.md`. |
