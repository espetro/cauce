# Tailwind hybrid migration of the SPA styles

Date: 2026-11-05. Branch: `v3/tailwind-hybrid` (from `v3/filter-primitives`),
worktree `~/.worktrees/cauce-tailwind-hybrid`.

## The mechanism that worked

Two variants, one root cause. `@apply` inside a Svelte `<style>` block
compiles fine through `@tailwindcss/vite` **when the block carries a
`@reference` line pointing at `app/app.css`** (`@reference "../app/app.css"`
relative to the component file). The build resolves it and emits the
expected utilities.

However svelte-check (v4.7.6) then reports two `Unknown at rule` warnings
per file (css source, codes `unknownAtRules` via its embedded VS Code CSS
language service). The `svelte-check ignore-start/next-line` comments
only suppress TypeScript diagnostics, not CSS ones, and the `--compiler-
warnings` flag does not reach CSS lint rules either; `--diagnostic-sources
"js,svelte"` would hide them but that is a gate-weakening change to
`package.json` this branch did not want to make unilaterally.

The working pattern, verified per component group with a real
`pnpm run build:spa`:

1. **`ui/` primitives** (whose classes were already `:global()`): the
   component keeps its semantic class names, the rules move to a sibling
   `<name>.css` file imported from the `<script>` block
   (`import "./button.css";`). The css file starts with
   `@reference "../app/app.css";` and composes each class with `@apply`.
   A script-level import bypasses Svelte scoping, which is exactly what
   the previous `:global()` did.

2. **`features/`, `ai/`, `app/`, `routes/`** (whose classes were
   component-scoped): the style block becomes
   `<style> @import "./X.css"; </style>` and the css file carries the
   `@reference`. This is the key discovery of the task: **`@import` inside
   a Svelte style block is inlined by the build, resolves `@apply` through
   the imported file's `@reference`, and preserves Svelte's scoping
   semantics exactly** — local classes still get the `.svelte-<hash>`
   suffix, verified in the emitted bundle. Svelte-check stays at zero
   warnings because its CSS parser never sees `@apply`/`@reference` in the
   style block itself.
   The four `ai/` leaf components (`loader`, `text-shimmer`, `steps`,
   `source-chip`) used `:global()` and no script imports; they follow
   variant 1 (script-level import) because their classes are used across
   component boundaries.

## Converted

`ui/`: button, select, checkbox, tooltip, dropdown-menu, tabs,
toggle-group, collapsible, dialog, alert-dialog, multi-select,
data-table (12 of 13; `switch.svelte` is on the do-not-modify list).
`features/`: AuditTab, CacheTab, EnginesTab, SourcesPanel, ArchiveView,
DashboardView, DayChart, HistoryView, ThreadsSection, Landing, FilterBar,
Omnibox. `ai/`: prompt-input, prompt-input-textarea, prompt-input-actions,
loader, text-shimmer, steps, source-chip. `app/`: TopNav,
ConversationShell. `routes/`: SearchPage. Total: 33 components.

Each converted component is now a hybrid: semantic class name kept as the
component API, utilities composed with `@apply` in its own stylesheet,
one-off presentational details left as raw declarations where no exact
Tailwind equivalent exists (see "deliberately raw" below).

## Class-name API preserved

All public class names are unchanged: `ui-button`, `ui-select-*`,
`ui-multi-select-*`, `ui-checkbox(-mark)`, `ui-toggle-*`, `ui-tabs-*`,
`ui-dialog-*`, `ui-alert-dialog-*`, `ui-dropdown-*`, `ui-tooltip-*`,
`ui-collapsible-*`, `dt-*` (data-table), plus every feature-local class.
Grep confirmed no other component references a name that moved.
SettingsView.svelte (skipped) still resolves its `ui-*` dependencies
because those classes are unchanged and still emitted globally.

One intentional scoping note: `ui/data-table`'s `.dt-*` selectors were
component-scoped in the original (Svelte hash) and are global after the
move to variant 1. Checked before the change: no other file uses `dt-*`,
so behaviour is identical; the emitted selectors simply lost the hash.

## Deliberately left as raw CSS (no exact utility)

- `font: inherit` (preflight already provides it on controls; keeping the
  declaration is byte-equivalent to the baseline).
- `overflow-wrap: anywhere` (no exact Tailwind utility; `break-anywhere`
  does not exist in v4.3.3).
- `border-left: none` / side-specific `border-*-color` mixes.
- `border-radius: calc(var(--radius) - 0.2rem)` and
  `calc(var(--radius) * 1.5)` — arbitrary-value `rounded-[...]` chokes on
  the unquoted `calc(...)` (the parser splits at the first space), so
  these stay as declarations.
- `transform: translate(-50%, -50%)` on dialogs: Tailwind's
  `-translate-x-1/2` emits the `translate` property, which would
  double-translate during the scale-in keyframe. Kept raw.
- `background-size: 200% auto` (Tailwind's arbitrary length drops `auto`).
- `font-size: 0.75rem` / `0.8125rem` / `0.9375rem` where the type scale
  has no matching step — raw declaration instead of `text-xs`/`text-sm`,
  because Tailwind's text-size utilities also emit a `line-height` the
  originals did not have (this bit `AuditTab`'s `.chip` and `ai/steps`
  during verification and was reverted to the raw declaration).
- `border-radius: 999px` / `50%`: `rounded-full` emits `2147483647px`,
  not `999px`, and no utility emits `50%` for non-square use — kept raw.
- keyframes, `transition:` shorthand lists, `z-index: 51` (no `z-51`
  utility), `inset-inline-end`, `touch-action`, `accent-color` (has a
  utility but the token form was kept where it read clearer), and all
  `@media (prefers-reduced-motion)` overrides.

## Verification

Baseline captured before any change:
`/tmp/cauce-tailwind-hybrid-baseline.css` (sha256 `c2749fec…`), from
`pnpm run build:spa` on the untouched worktree.

- `pnpm run build:spa` — green after every component commit (14 runs).
- `pnpm run check:spa` — 0 errors, 0 warnings (the gate is "no new
  warnings"; the baseline had 0).
- `pnpm run typecheck` — clean.
- `pnpm run test` — 36/36 pass (2 files), same as baseline.
- CSS equivalence: the emitted bundle was parsed selector-by-selector and
  compared against the baseline with spacing-scale arithmetic normalised
  (`calc(var(--spacing,.25rem) * n)` vs `n*0.25rem`), Tailwind `--tw-*`
  plumbing variables stripped, and Svelte hash suffixes removed. The only
  residual differences are equivalent spellings (`padding-block` /
  `padding-inline` vs `padding`, keyframe names losing their
  `svelte-<hash>` prefix because the components now import real files).
  The keyframe renames are safe: all eight `ui-*`/`ai-*` keyframe names
  are unique globally (grep-checked; no collision).
- One regression was caught and fixed during verification rather than
  shipped: Tailwind's `-translate-x-1/2`/`-translate-y-1/2` on
  `ui/dialog` would have replaced `transform: translate(-50%,-50%)` with
  the `translate` property and broken the dialog's centre-on-screen
  positioning during the scale animation.

## Not converted (deliberate)

- `ui/switch.svelte`, `features/settings/SettingsView.svelte`,
  `routes/HomePage.svelte` — on the do-not-modify list (another agent
  owns them).
- `app/app.css` — untouched, per constraint.
- No `.rs` file, nothing under `locales/`, no `assets/spa/` artifact was
  committed (build output was regenerated locally to verify and then
  restored; `git status` is clean).

## Risks for the parent branch to check on merge

1. **rust-embed freshness gate**: `mise run web` ends with
   `git diff --exit-code -- assets/spa …`. This branch did not commit
   `assets/spa/` (it belongs to the parent's filter-primitives work), so
   the parent must rebuild after merging or the gate will fail on a stale
   bundle.
2. **svelte-check warning budget**: the whole mechanism relies on
   `@apply` never appearing inside a `<style>` block. If someone adds
   `@apply` directly to a style block later, two warnings come back. The
   comment in each imported css file and in the `ui/*.svelte` import sites
   states the rule; consider a lint rule later.
3. **`text-*` utilities add `line-height`**: the type-scale utilities are
   not drop-in replacements for raw `font-size`. This bit two files during
   verification. Any future `@apply text-sm` on an element that relied on
   the inherited line-height will change rhythm — the same trap the
   `app.css` type-scale comment warns about.
4. **`rounded-full` is not `999px`**: Tailwind emits `2147483647px`.
   Visually identical for pill shapes but not byte-equal; anything that
   screenshots pixel-diffs at extreme zoom or parses the CSS will see it.
5. **`ai/source-chip` is dead code** (imported nowhere); its css now ships
   unused rules. Pre-existing, unchanged by this branch.
