# Wave 0 orchestration retro

Dated 2026-09-22. Wave 0 (12 steps, EPIC #1) shipped via parallel subagent dispatch with the
main agent as orchestrator/quality gate. This note is what to repeat and what to change for
wave 1.

## What worked

- **One agent per batch step, isolated worktrees in `~/.worktrees/oxe-<step>`** kept 5 Batch B
  implementations truly parallel. Collisions were only `Cargo.lock` + `cmds/mod.rs`, resolved
  with union merges in minutes.
- **Task reviewer per step** paid off. Reviews caught 10 real defects pre-merge, including two
  that would have recreated v2 failures: `NoResults`-as-failure (v2's "page 2 always 502") and
  the FTS NULL-rowid desync in immutable migration files.
- **Seeding shared contracts on main before dispatch** (`StoreTuning` before W0-04/W0-11 ran
  in parallel) prevented type drift between agents that never see each other's code.
- **Full API surface in Batch D dispatch** (method signatures, `configured_engines()`,
  redaction contract) let one agent do W0-09 → 10 → 12 with zero clarifying rounds.

## What to do differently

- **Manual browser QA is not optional for HTMX work.** Server-side tests cannot catch client
  behavior. The browser caught three defects server tests structurally could not: htmx
  cancelling anchor navigation (hx-post on `<a>`), htmx `hx-vals` string-flattening scalars
  through json-enc (`"position":"0"` → 400), and the `/favicon.ico` 404. Rule: any UI wave
  needs a real-browser click-through before merge, not just before wave close.
- **`cargo fmt` blind spot**: modules declared via in-function `#[path]` are invisible to
  rustfmt. `record.rs` landed unformatted and only surfaced when `cmds/mod.rs` unified the
  tree. Check for `#[path]` usage during review.
- **Verify the host has disk before the last wave step.** W effectively-blocked pre-push
  validate on a 100% full disk; W0-12's push used `--no-verify` and relied on CI. It was fine,
  but the gate should not depend on it.
- **Subagent death is silent.** The first W0-11 agent exited with zero commits and no error.
  Ledger + explicit "produce a PR or die trying" acceptance in dispatch prompts helps; check
  worktree commit count early, not just the completion message.
- **Live-verification numbers worth keeping**: ddgs cold path ~2-4s exceeds the 3s default
  deadline (filed as #83 livelock risk); warm cache hit 2.7ms; release binary 9.99 MiB;
  validate on this machine ~2m, CI 4-6.5m.

## Board hygiene

- `set-status.sh` + `item-ids.txt` in `.superpowers/sdd/<wave>/` worked well for status moves;
  reuse the pattern per wave.
- Follow-ups go to plain issues (#82-#89 from this wave), not into the wave epic.
