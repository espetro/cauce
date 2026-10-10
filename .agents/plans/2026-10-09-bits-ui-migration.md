# Bits UI migration — the SPA design system

Status: ready for execution. Branch `v3/bits-ui` (off `main` @ 35716b8).
Scope: `crates/cauce-server/web/src/spa/` — every interactive surface.
Parent direction: `2026-09-28-frontend-replacement.md` §7.3 ("Bits UI
(headless, Svelte 5) + owned `ui/` wrappers", the shadcn-svelte vendoring
model) — applied to the whole SPA, not just the settings form.
`bits-ui@^2.19.5` is already a devDep; `ui/select.svelte` +
`ui/checkbox.svelte` are the vendored seeds this plan extends.

Aesthetic north star: a quiet reading room, not a console — links-first
results on hairlines (Kagi/searXNG simple theme), a boxed composer with a
tool row (Perplexity/ai-elements), avatar-free full-width answers with
superscript citations, Linear-grade density on admin/settings. One
accent, hairlines not shadows, three radius tiers, 150–200 ms ease-out
motion, mobile-first with the centered ≤45rem desktop column. Dense
tool aesthetic — not a marketing page; no hero whitespace, no bento,
no scroll choreography.

## 1. Design-system architecture

### 1.1 The `ui/` layer (house conventions — extend, don't reinvent)

- One file per wrapper: `ui/<name>.svelte`, imported PascalCase
  (`UiSelect`, `UiCollapsible`). No per-component folders or barrel
  files — the seed convention is single-file wrappers.
- Wrappers compose Bits UI parts; they never re-implement behavior
  (no hand-rolled keyboard nav, focus traps, or floating positioning —
  `@floating-ui/dom` is already in the lockfile via bits-ui).
- One-way data flow: `value`+`onValueChange`, `checked`+`onCheckedChange`,
  `open`+`onOpenChange`, `pressed`+`onPressedChange`. This is what
  formisch `field.onInput` plugs into. Never mix `bind:x` and
  `onXChange` on the same prop (double-fires); `bind:` alone is allowed
  only for purely-local state.
- Styling contract: plain `<style>` blocks with `:global(.ui-<name>-*)`
  classes keyed on the app tokens (`--bg --fg --muted --border --accent
  --greyed-bg --warn --radius`, plus the new §1.3 tokens). No Tailwind
  utilities inside `ui/` files; no hex/rgb/oklch literals anywhere
  outside `app.css`. `:global` is load-bearing: portaled floating
  content escapes scoped ancestors and is still styled by it.
- Forward the field props every wrapper gets: `name`, `id`, `disabled`,
  `required`, `ariaInvalid` → `aria-invalid`, plus `aria-label` where the
  part takes one.
- Composition: prop-driven (`options: Option[]`, `tabs: Tab[]`) for
  data-shaped widgets; `{#snippet}` composition where callers own
  content (collapsible trigger/body, menu items, dialog body).
- Every new wrapper file carries the MPL-2.0 header + a doc comment
  naming the bound Bits parts and every deliberate deviation (same
  pattern as the two seeds).
- New wrappers only with a real consumer — no speculative components.

### 1.2 Forbidden raw / allowed raw

FORBIDDEN without a `// justified:` comment in-file or a §3 entry:

- `<details>`/`<summary>` as disclosure or dropdown widget
- `<select>`, `<input type="checkbox">`, `<input type="radio">`
- `aria-pressed`/`aria-expanded` state buttons; hand-rolled tab bars,
  menus, popovers, tooltips, dialogs, toggles, accordions
- `window.confirm` / `window.alert` / `window.prompt`
- `<button>` outside `ui/` — the design-system button is `UiButton`;
  a raw `<button>` in a feature file is a defect (Bits `Button.Root`
  covers the href-less case; it renders `<a>` when given `href`)
- any bespoke open/close state machinery duplicating a primitive

ALLOWED raw (semantic elements, no widget behavior):

- `<a>`, `<p>`, `<div>`, `<span>`, `<ul>/<ol>/<li>`, `<table>`,
  `<form>`, `<label>`, `<fieldset>/<legend>`, headings
- text-like inputs: `type=text|search|password|number|email|url`,
  `<textarea>` — plus formisch `Form`/`Field` plumbing
- `tabindex="0" role="region"` overflow scroll regions (§3.6)
- `aria-live`/`role=status`/`role=alert` regions — passive a11y, not
  widgets

### 1.3 Token & motion rules (`app/app.css`)

New tokens land in DS-00, declared beside the existing set in both the
light `:root` and the dark blocks (`prefers-color-scheme` +
`[data-theme="dark"]`):

- `--overlay` — dialog/menu scrim
- `--shadow` — the single floating-layer shadow recipe
  (`0 4px 12px rgb(0 0 0 / .12)`, darker in dark mode)
- `--ease-out`, `--ease-in` — cubic-bezier curves; enter = ease-out,
  exit = ease-in, plain `ease` only for hover color/opacity

Rules:

- Radius vocabulary is three tiers: `--radius` (controls, rows),
  `calc(var(--radius) * 1.5)` (composer, dialogs), `999px` (chips,
  pills, segments). Nothing else.
- Shadow budget: floating layers only (dropdown content, popover,
  dialog, tooltip). Static surfaces get hairline borders — never
  shadows.
- Durations: 120–200 ms micro/state changes, ≤250 ms floating content,
  ≤300 ms dialogs. `>300ms` needs a `// justified:` comment.
- Compositor properties only (`opacity`, `transform`, `color*`);
  disclosure height animation uses `--bits-collapsible-content-height`,
  not layout-property transitions. Never `transition: all`.
- DS-00 extends the existing `prefers-reduced-motion` body rule into a
  global guard covering `ui/` + feature transitions/animations.
- Focus ring is one recipe: `outline: 2px solid var(--accent);
  outline-offset: 1px`. Never bare `outline: none` (the composer input's
  `focus-within` parent styling is the sole existing exemption).
- Icon buttons reach ≥28px hit area via padding; ≥36px on the mobile
  nav row.
- The residual SSR pages (`/trace/{id}`, `/answer/{id}`) do not render
  floating layers, so `src/pages/mod.rs`'s token mirror needs no new
  tokens; only touch it if a new token ends up referenced there.

## 2. Wrapper inventory

Already vendored: `ui/select.svelte` (with the `""`⇄`"__none__"`
sentinel — keep it), `ui/checkbox.svelte`. To build:

| Wrapper | Bits parts | Public API | Consumers |
|---|---|---|---|
| `ui/button` | `Button.Root` | `variant?: "default"\|"primary"\|"danger"\|"ghost"`, `size?: "md"\|"sm"\|"icon"`, `type`, `disabled`, `onclick`, `href?` (renders `<a>`) | every action/submit site (~13 files) |
| `ui/collapsible` | `Collapsible.Root/Trigger/Content` | `open`, `onOpenChange`, `disabled`, `onOpenChangeComplete` (for lazy-load-on-open), `trigger` + `children` snippets | 6 files (all `<details>` sites) |
| `ui/dropdown-menu` | `DropdownMenu.Root/Trigger/Portal/Content/Item/Separator` | trigger prop or snippet; items composed via `Item` `child` snippet rendering `<a>` (keeps link semantics) or `onSelect`→`navigate()` | `TopNav` `.nav-more` |
| `ui/toggle-group` | `ToggleGroup.Root/Item`, `type="single"` | `value`, `onValueChange`, `options: {value,label,disabled?}[]` | Omnibox segment, dashboard `?days=` |
| `ui/tabs` | `Tabs.Root/List/Trigger` | `value`, `onValueChange`, `tabs: {value,label}[]`; URL-driven — caller maps `onValueChange`→`navigate()`, panes render via the route (no `Tabs.Content` needed) | `AdminView` `?tab=` |
| `ui/alert-dialog` | `AlertDialog.Root/Portal/Overlay/Content/Title/Description/Action/Cancel` | composed by `ui/confirm`, not used directly | — |
| `ui/confirm` (host + helper) | — | `confirm({title, description, confirmLabel?, danger?}): Promise<boolean>`; `<UiConfirmHost>` mounted once in `App.svelte`; call sites read `const ok = await confirm({...})` — drop-in for `window.confirm` | 5 confirm sites across 4 stores |
| `ui/tooltip` | `Tooltip.Provider/Root/Trigger/Portal/Content` | Provider mounted once in `App.svelte` (`delayDuration` ~300 ms); wrapper exposes `content`, `side`, and a `trigger` snippet | theme cycle, `nav-more` trigger, optional `title=` upgrades |

Explicitly NOT built (no consumer or justified keep-raw): `ui/input`
(text inputs stay raw per the rules), `ui/label`, `ui/separator`,
`ui/radio-group`, `ui/switch` (all booleans are in-form → `UiCheckbox`
is the right primitive, not Switch), `ui/combobox`, `ui/command` (the
⌘K palette is a new feature — separate plan), `ui/pagination` (§3.5),
`ui/scroll-area` (§3.6), `ui/accordion`, `ui/popover`, `ui/avatar`,
`ui/link-preview`, `ui/slider`, `ui/progress`, `ui/dialog` (the only
dialog need is destructive-confirm → AlertDialog).

## 3. Settled decisions (the flagged behaviors, resolved)

1. **`window.confirm` ×5** (`settings.svelte.ts`,
   `cache.svelte.ts` ×2, `archive.svelte.ts`, `history.svelte.ts`) →
   `ui/confirm` backed by AlertDialog. Adds i18n keys `common.cancel`
   and `common.confirm` (en.yaml + `spa_bundle()` allowlist +
   `gen_i18n`); per-site description strings already exist
   (`cache.confirm_*`, `archive.confirm_row`, `history.delete_confirm*`,
   `settings.confirm_*`). Native confirm is a hand-rolled overlay —
   banned by the brief.
2. **`<details>` disclosures** → `UiCollapsible`. Lazy-load sites
   (archive/cache row payloads) rewire `ontoggle` →
   `onOpenChange`/`onOpenChangeComplete`; history rows keep controlled
   `open`.
3. **`TopNav` `.nav-more`** → `UiDropdownMenu` with `child`-snippet
   `<a>` items (link semantics + keyboard nav + esc/outside-close for
   free).
4. **URL-driven selection** (`AdminView` `?tab=`, `DashboardView`
   `?days=`): primitives controlled by the URL value —
   `value={s.tab} onValueChange={(v) => navigate(href(v))}`. The URL
   stays the source of truth (deep links preserved); the primitive
   supplies tablist/segment semantics and arrow-key nav.
5. **Pagers stay prev/next links.** They are navigations with deep
   links (`?offset=`), not page-local state; a numbered Pagination
   adds chrome for zero gain. Justified exception.
6. **`tabindex="0"` scroll regions** (AuditTab table wrap + `<pre>`)
   stay native — already keyboard-scrollable; ScrollArea would be a
   styled scrollbar for no a11y gain. Justified exception.
7. **Theme tri-state cycle** stays a single `UiButton` — `Toggle` is
   binary, a 3-state cycle is not a toggle-group either. Justify
   in-file; gains `UiTooltip` per §2.
8. **`AssistCard` one-shot reveal** stays bespoke — the reveal never
   collapses, so `Collapsible` semantics don't fit and a forced-open
   primitive is worse than a plain conditional swap. Justify in-file.
9. **Infrastructure, not widgets** (justify in-file where a reader
   would wonder): `App.svelte` delegated `a[href]` click router +
   `popstate`; `AnswerPage`/`AdminPage` scroll-follow/`scrollIntoView`;
   `{@html}` + `a.cite[data-cite]` retargeting in `AnswerTurn`/
   `AssistCard`.
10. **`Omnibox` Search/✦Ask segment** → `UiToggleGroup` (single) —
    exact aria-pressed-pill match, plus roving focus.
11. **`Omnibox` send** stays `type="submit"` but becomes a compact
    `UiButton` (`size="icon"`, `variant="primary"`), disabled while
    the trimmed query is empty.
12. **`DayChart` `<title>` hovers** may upgrade to `UiTooltip` —
    optional polish inside DS-10; `<title>` is already acceptable
    a11y-wise.

## 4. Task plan

Groups are parallel cohorts; tasks inside a group have disjoint file
sets and may run concurrently. Groups run in order — foundations first.
Every task runs the fast gates from `crates/cauce-server`:
`pnpm run check:spa`, `pnpm run typecheck`, `pnpm run build:spa`,
`pnpm run test`.

Paths below are relative to `crates/cauce-server/web/src/spa/` unless
rooted.

### Group 0 — foundations (3 parallel)

- **DS-00 tokens + `ui/button`** — `app/app.css`, `ui/button.svelte`.
  Add `--overlay`, `--shadow`, `--ease-out`, `--ease-in` in all theme
  blocks; global `prefers-reduced-motion` guard; vendored `UiButton`
  with `default|primary|danger|ghost` variants and `md|sm|icon` sizes.
- **DS-01 disclosure + selection wrappers** — `ui/collapsible.svelte`,
  `ui/toggle-group.svelte`, `ui/tabs.svelte`, `ui/tooltip.svelte`.
- **DS-02 overlay wrappers + confirm plumbing** —
  `ui/dropdown-menu.svelte`, `ui/alert-dialog.svelte`, `ui/confirm.ts`,
  `app/App.svelte` (mount `<UiConfirmHost>` + `Tooltip.Provider`),
  `crates/cauce-server/locales/en.yaml`,
  `crates/cauce-server/src/i18n.rs`, `crates/cauce-server/web/src/i18n/*.json`
  (regenerated via `cargo run -p cauce-server --bin gen_i18n`).

### Group 1 — shell, search, answer (3 parallel)

- **DS-03 app shell** — `app/TopNav.svelte` (`nav-more` →
  `UiDropdownMenu`; theme cycle → `UiButton` + `UiTooltip`),
  `app/GateBlock.svelte` (buttons → `UiButton`).
- **DS-04 search surface** — `features/search/Omnibox.svelte`
  (segment → `UiToggleGroup`; send → icon `UiButton`, disabled when
  empty), `routes/SearchPage.svelte` (`meta-detail` → `UiCollapsible`;
  More → `UiButton`).
- **DS-05 answer surface** — `features/answer/AnswerTurn.svelte`
  (steps → `UiCollapsible`; `turn-edit` → ghost `UiButton`),
  `features/answer/AssistCard.svelte` (trigger → `UiButton` +
  justified one-shot reveal), `features/answer/FollowupComposer.svelte`
  (submit/stop → `UiButton`), `routes/AnswerPage.svelte` (justify
  comments for scroll-follow hooks only).

### Group 2 — admin (3 parallel)

- **DS-06 admin shell + engines** — `features/admin/AdminView.svelte`
  (`?tab=` anchors → `UiTabs` driving `navigate()`),
  `features/admin/EnginesTab.svelte` (all buttons → `UiButton`;
  danger variant for destructive ops).
- **DS-07 audit tab** — `features/admin/AuditTab.svelte` (2 selects →
  `UiSelect`; per-cell `<details>` → `UiCollapsible`; submit →
  `UiButton`; scroll regions stay, justified).
- **DS-08 cache tab** — `features/admin/CacheTab.svelte`
  (row `<details>` → `UiCollapsible` with lazy payload load; buttons →
  `UiButton`), `features/admin/cache.svelte.ts` (2 `window.confirm` →
  `confirm()`).

### Group 3 — read surfaces + settings (3 parallel)

- **DS-09 history** — `features/history/HistoryView.svelte` (2 selects →
  `UiSelect`; checkbox → `UiCheckbox`; clicks `<details>` →
  `UiCollapsible`; delete/submit → `UiButton`),
  `features/history/history.svelte.ts` (`window.confirm` → `confirm()`).
- **DS-10 archive + dashboard** — `features/archive/ArchiveView.svelte`
  (row `<details>` → `UiCollapsible` lazy-load preserved; buttons →
  `UiButton`; pager stays links), `features/archive/archive.svelte.ts`
  (`window.confirm` → `confirm()`),
  `features/dashboard/DashboardView.svelte` (`?days=` links →
  `UiToggleGroup` driving `navigate()`),
  `features/dashboard/DayChart.svelte` (optional `UiTooltip`).
- **DS-11 settings** — `features/settings/SettingsView.svelte`
  (remaining raw `<button>` → `UiButton`; formisch/UiSelect/UiCheckbox
  stay), `features/settings/settings.svelte.ts` (`window.confirm` →
  `confirm()`).

### Group 4 — sweep (1 task)

- **DS-12 final sweep + committed assets** — whole `web/src/spa/` tree:
  assert zero raw interactive widgets; delete stale `*/.gitkeep`
  placeholders; remove dead CSS (`details.*`, `.filters select`,
  `table.data` select rules, `.filters button`, `.settings-form
  button`, `.tabs` leftovers absorbed by `ui/tabs`, `.row-actions
  button`); regenerate and commit `assets/spa/` + any `pnpm-lock`
  drift; run all four gates green.

### Group 5 — AI surfaces (1 task)

- **DS-AI AI-mode surfaces on an AI-chat foundation** —
  `web/src/spa/ai/` (new, vendored), `features/answer/*`,
  `features/search/Omnibox.svelte`, `routes/AnswerPage.svelte`,
  `routes/SearchPage.svelte`, `web/AGENTS.md` (features may also
  import `ai/`).

  **Foundation choice: sv-prompt-kit** (Svelte Prompt Kit —
  `sv-prompt-kit.vercel.app`, the lightweight registry sibling of
  Svelte AI Elements). Adopted the shadcn way the plan already
  uses for `ui/`: components vendored into `web/src/spa/ai/`,
  restyled on the app tokens, zero new npm deps (`runed`'s
  `watch` → `$effect`, lucide → `phosphor-svelte`, shadcn
  registry-deps → `ui/` wrappers). Its composer (PromptInput
  context + textarea + actions), loader, text-shimmer, steps and
  source-chip cover the whole scope — composer send/stop +
  disabled/submitting states, streaming indicator, sources
  rail, assist card — without owning the wire:
  `features/answer/thread.svelte.ts` keeps the SSE-over-POST
  plumbing verbatim.
  - why-not **TanStack AI** (`@tanstack/ai-svelte`): a data-layer
    chat client only — no presentation components, and its
    chunk/stream protocol would replace `thread.svelte.ts`
    rather than adapt to the named-frame SSE
    (`step`/`delta`/`sources`/`done`/`error`, sources held to
    `done`, stop-keeps-partial, `editLast` rewind).
  - why-not **Vercel AI SDK + ai-elements Svelte port**: the SDK
    is likewise a data layer expecting the UI-message stream
    protocol, and the registry's flagship blocks drag `ai@^6`,
    `streamdown-svelte`, `shiki`, `mode-watcher` for client-side
    markdown — dead weight when the server already renders and
    sanitizes `done.html`; heavier than the scope needs.
  Vendored pieces: `ai/prompt-input/` (context/root/textarea/
  actions), `ai/loader.svelte` (typing variant), `ai/
  text-shimmer.svelte`, `ai/steps.svelte` (composed on
  `UiCollapsible`), `ai/source-chip.svelte`. Consumers: Omnibox
  (composer + send; the mode segment stays `UiToggleGroup` —
  the foundation ships no segmented control), FollowupComposer
  (send/stop swap on `isLoading`), AnswerTurn (`AiLoader` +
  `AiTextShimmer` status, `AiSteps`), AssistCard (busy loader,
  `AiSourceChip` rail). `ui/` wrappers remain the primitives
  for everything the foundation does not cover.

## 5. Shared rules for every task

- Commits: Conventional Commits, atomic, `git commit -s`, MPL-2.0
  headers on new files, no AI trailers, author stays Quim.
- **`assets/spa/` is regenerated and committed only in DS-12** — hashed
  bundle names collide across parallel branches. Intermediate tasks
  still run `pnpm run build:spa` to prove the tree compiles, but keep
  `assets/` out of their commits (`git restore assets/` before
  committing).
- i18n: any new UI string goes to `locales/en.yaml` AND the
  `spa_bundle()` allowlist in `src/i18n.rs`, then `gen_i18n` — DS-02
  owns the dialog labels; other tasks may only reuse existing keys.
- `features/` import only `ui/` + `lib/`; `routes/` stay thin shells;
  named `interface`s for object types — no inline object literals.
- Every kept-bespoke behavior carries a `// justified:` comment.
- Verification beyond the gates: a validator should be able to check
  each rubric by grepping the tree and, where behavioral, by running
  `cauce serve` on the replay engine (`CAUCE_ENGINES=replay`) and
  driving `/app` at 390px and desktop widths in both themes.
