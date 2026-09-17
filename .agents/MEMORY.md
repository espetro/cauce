# Agent memory process (oxe)

Project-scoped agent memory with time-bounded recall ("dreaming"). Per the repo owner's global
memory policy (`~/MEMORY.md`), memory lives inside this repo under `.agents/`, never in a
global store, and is never shared with or copied into another project.

```
.agents/MEMORY.md          this file: process definition + compaction log (committed)
.agents/notes/              durable dated findings (committed)
.agents/docs/                permanent docs, incl. screens/ (committed)
.agents/drafts/              raw intermediate research (gitignored)
.agents/plans/                implementation plans (tracked here — see root .gitignore's
                              `!.agents/plans/` override)
```

## Process

1. **Session start**: read this file, then skim `.agents/notes/` newest-first for anything
   relevant to the task at hand.
2. **Session end** (or before context compaction): write durable findings as
   `.agents/notes/YYYY-MM-DD-<topic>.md`. If a new note supersedes an older one, mark the old
   note with `Superseded by <note>.` at its top rather than deleting it outright.
3. **Periodic dream** (notes directory > ~15 files, or each milestone): merge same-topic note
   series into one note, delete notes superseded more than one milestone back, archive stale
   `.agents/drafts/` and `.agents/plans/` entries, and update the compaction log below.

## Compaction log

- **2026-09-17 — reset.** `main` was orphaned from the 54-commit `feat/v0.4.0-web-ui`
  exploration (now `legacy`; see `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`, Wave 0).
  `legacy`'s `.agents/MEMORY.md` held four dated entries of v0.4.0 implementation detail (AI
  pipeline internals, shipped-feature summary, P0 fixes, a six-issue UI audit) plus one
  2026-09-17 entry with the v0.5.0 planning research and a velocity retrospective. The
  implementation-detail entries describe code that no longer exists in this tree and were not
  carried forward. The retrospective findings were carried forward into
  `.agents/notes/2026-09-17-velocity-retro.md` — read that note for what the last attempt's
  cost actually was (11,565 lines of retrofit churn, zero CI runs, the specific bugs that
  motivated each new gate) before re-deriving any of it from scratch.
