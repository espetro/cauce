# Wave 2 merge-pipeline lessons (2026-09-23)

Durable findings from landing 15 parallel Wave-2 workstreams.

## Squash merges can silently drop merge-commit resolutions

`gh pr merge --squash` re-performs the merge; semantic fixes made while resolving
conflicts in a merge commit (e.g. a new required struct field added to a call
site that exists on both sides) can be lost. #138's squash dropped
`HistoryFilter.cached: false` and left main uncompilable until ce928e8.
**Pipeline rule now: `cargo check` on main after every squash merge.**

## Shared CARGO_TARGET_DIR across worktrees poisons builds

Unchanged-by-merge files keep old mtimes; cargo reuses newer artifacts from
sibling worktrees compiled against different source. Symptom: tests assert
fields/routes that exist in source but "don't compile", or tests serve stale
templates. Fix: `cargo clean -p <crates>` in the shared target before the gate,
or accept sequential full rebuilds. Sibling-check so far: failures that vanish
on clean rebuild are this, not real bugs.

## Env-mutating tests need env_lock + CAUCE_CONFIG_DIR

Route probes that hit POST /api/engines/{id}/enable write config.toml. Tests
must hold the per-binary ENV_LOCK and point CAUCE_CONFIG_DIR at a tempdir or
they corrupt the owner's real config.

## tracing-core callsite Interest is process-global

`Interest::never` cached by a dispatcher-less thread disables the span for all
subscribers permanently. Tests needing spans must `set_global_default` +
`tracing::callsite::rebuild_interest_cache()`.

## UI eval loop discipline that worked

Screen specs per page (.agents/docs/screens/), axe 0 serious at 1280+375 on
EVERY page (a shared-header change means every page is in scope), scrollWidth
== clientWidth at 390/375, playwright-cli only, eval server rebuilt after each
fix round (CSS/templates are compile-time).

## Deadline tests must prove the child is alive first

exec-engine tests spawn real python3 fixtures. Under the parallel gate a
transient spawn/boot failure resolves inside the test's deadline window
and fails as Transport, masquerading as a deadline regression. Fix
(05e42ea): warm_child() answers a fast query with Transport-only retries
before the timed section; the lazy/eager spawn tests retry across fresh
engine instances so persistent failure still shows the real EngineError.
Retries of the SAME engine would mask eager-vs-lazy regressions.
