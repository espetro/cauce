# Keeping Rust worktree builds small

Repo-agnostic playbook for projects where an orchestrator runs several agents in parallel git
worktrees (for example `~/.worktrees/<project>-<task>`). Written for macOS/APFS but each section
notes the portable equivalent. Motivating incident: five parallel worktrees, each building its
own multi-GB `target/`, pushed a 228 GB SSD to 100% mid-task and blocked the pre-push gate.

## The short version

1. Slim the dev profile in the workspace `Cargo.toml` (committed).
2. Give the host a shared rustc cache via sccache (host config only, never committed).
3. Seed each new worktree's `target/` with an APFS clone of the main checkout's.
4. Skip incremental compilation for one-shot gate builds.
5. Delete worktrees at merge time, and check `df` before dispatching a wave.

Each is independent; adopt in order.

## 1. Slim the dev profile (commit this)

Debuginfo is typically the majority of `target/debug`. Most projects never consume DWARF
(e2e tests that spawn the binary, panic `file:line` from `Location` metadata, and CI logs all
work without it). In the workspace-root `Cargo.toml`:

```toml
[profile.dev]
debug = 1                    # line tables only, panic traces keep line numbers

[profile.dev.package."*"]
debug = 0                    # dependencies get no debuginfo at all
```

Expected: roughly half or more off `target/debug`. Release profile untouched. Escape hatch for
a debugging session: `CARGO_PROFILE_DEV_DEBUG=2 cargo test` (and
`CARGO_PROFILE_DEV_PACKAGE__ALL__DEBUG=2` for deps).

Before adopting, verify nothing reads debug symbols: grep for `backtrace`, `RUST_BACKTRACE`
workflows that need dep frames, or debugger-based tooling.

## 2. Host-level sccache (do NOT commit)

A shared `CARGO_TARGET_DIR` looks like the obvious fix but is wrong for this workflow: Cargo
holds a build-dir lock, so N parallel agents serialize into a queue. sccache gives the same
artifact reuse without the lock, and its cache (`~/Library/Caches/sccache`) survives worktree
deletion, which is exactly what short-lived agent worktrees need.

Host setup (mise shown; any install works):

```bash
mise use -g sccache
# then in global mise config [env] or shell rc:
#   RUSTC_WRAPPER = "sccache"
```

Never put `rustc-wrapper` in a committed `.cargo/config.toml`: it changes artifact fingerprints
for every checkout, so CI or a bare `cargo` without sccache either rebuilds everything or fails.
Host env only. Keep CI on its existing cache (e.g. `Swatinem/rust-cache`); sccache adds nothing
there.

Caveats: sccache cannot cache incremental compiles (pair with section 4), proc-macro crates and
final bin links are not cached, and C builds (e.g. `*-sys` crates with `bundled`) bypass it
unless `CC`/`CXX` are also wrapped.

## 3. Clone-seed target/ on APFS (dispatch convention)

APFS clonefile gives dedup for free: after `git worktree add`, copy the main checkout's
`target/` with `cp -Rc`:

```bash
git worktree add ~/.worktrees/proj-task -b task/branch main
cp -Rc /path/to/main-checkout/target ~/.worktrees/proj-task/target
```

Physical cost is near zero (clones share extents until written), the worktree starts with warm
dependency artifacts, and there is no lock. Registry-dep artifacts are path-independent so they
stay fresh; workspace-crate artifacts carry path-keyed fingerprints and rebuild, which is
correct. Spot-check once per project: build in the seeded worktree and confirm the workspace
members recompile. If stale artifacts ever leak through, `cargo clean -p <workspace crates>`
post-seed is cheap and the stale clones cost no disk.

Non-APFS hosts: `cp -Rc` falls back to a plain copy (still useful as a warm start, just not
free). There is no equivalent zero-cost seed on ext4; consider `--reflink=auto` which picks up
btrfs/xfs CoW where available.

## 4. Skip incremental for one-shot builds

`target/debug/incremental/` accumulates artifacts for workspace/path-dep crates on every
compile. Agent worktrees build once and are deleted, so incremental buys nothing. Options:

- Per-task env on the CI/gate tasks rather than repo-wide, so day-to-day `cargo run` keeps
  incremental:

  ```toml
  [tasks.test]   # mise task syntax
  env = { CARGO_INCREMENTAL = "0" }
  ```

- Or export `CARGO_INCREMENTAL=0` in the environment the orchestrator dispatches agents with.
  Do not set it repo-wide in `.cargo/config.toml`; it slows editor/`cargo watch` iteration.

## 5. Teardown and pre-dispatch hygiene

- `git worktree remove <path>` immediately when a PR merges; worktree deletion is the 100%
  reclaim and needs no sweep tool. `git worktree list` in the main checkout audits leftovers.
- Orchestrators: run `df -h /` before dispatching a parallel batch; if headroom is under a few
  times the expected per-worktree `target/` size, clean first.
- For the main checkout, an occasional `cargo clean` beats installing a sweep tool at this
  scale.

## 6. Watch for nested builds

Anything that shells out to `cargo` with its own `CARGO_TARGET_DIR` inside `target/` (a common
pattern for "build a release binary in a test") hides a second dep-graph build inside every
checkout. Point it at a shared cache dir instead (`$XDG_CACHE_HOME`/`~/.cache`/`~/Library/Caches`),
with an env override so CI can keep it under the cached `target/`:

```rust
let target_dir = std::env::var_os("OXE_BUDGET_TARGET_DIR")   // your own var name
    .map(PathBuf::from)
    .unwrap_or_else(|| /* ~/.cache/<proj>/e2e-budget */ ...);
```

## What not to do

- Shared `CARGO_TARGET_DIR` across worktrees: the build lock serializes parallel agents and
  diverging lockfiles thrash feature unification.
- Committed `rustc-wrapper`/`sccache` in `.cargo/config.toml`: fingerprint mismatch for anyone
  without it, CI included.
- tmpfs/ramdisk for `target/` on small-RAM hosts: volatile, eats RAM or swap, and macOS needs
  `hdiutil ram://` plumbing for a benefit APFS already gives via clones.
- A periodic `cargo-sweep` habit as a substitute for deleting finished worktrees.

## How this repo applies it

- `Cargo.toml` carries the section-1 profile; `mise.toml` sets `CARGO_INCREMENTAL=0` on the
  lint/test gate tasks; `budget.rs` honors `OXE_BUDGET_TARGET_DIR` (CI pins it under `target/`,
  locally it defaults to `~/.cache/oxe/e2e-budget`).
- Host setup for this machine: `mise use -g sccache` and `RUSTC_WRAPPER=sccache` in the global
  mise `[env]`.
- Orchestrator dispatch rules live in `.agents/plans/v3/README.md` (clone-seed, `df` check,
  remove-on-merge).
