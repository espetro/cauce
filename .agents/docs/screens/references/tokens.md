# Tokens: machine-readable design values

The single source of truth for numeric/enum design values used across the
other `patterns-*.md` docs in this directory. Other docs should link here
instead of restating a number; if a doc and this file disagree, this file
wins and the other doc is stale.

Every token below is mapped to its daisyUI 5 equivalent where one exists.
cauce's `ui/src/index.css` currently declares only `@plugin "daisyui" { themes:
light --default, dark --prefersdark; }` with no theme overrides, so cauce runs
on daisyUI's unmodified built-in `light` / `dark` themes. Anything marked
"daisyUI default" below is inherited, not chosen; changing it means adding
an override block to `index.css`, not just picking a different token name
in a component.

## Applies to

All checkpoints in `userflow-checkpoints.md` (this doc underlies every
visual surface). Directly consumed by `patterns-typography.md`,
`patterns-motion.md`, `patterns-layout-grid.md`, `patterns-result-list.md`,
`patterns-search-input.md`, `patterns-states.md`, `patterns-hybrid-serp.md`.

## Type scale

See `patterns-typography.md` for the full rationale and per-product
citations. The scale itself, canonical here:

| Role | Size | Weight | Line height | daisyUI / Tailwind class |
|---|---|---|---|---|
| Display hero | 40-56px (2.5-3.5rem) | 500-600 | 1.1 | `text-4xl md:text-5xl font-semibold` |
| Heading large | 24-28px | 600 | 1.3 | `text-2xl font-semibold` |
| Heading | 20-22px | 600 | 1.35 | `text-xl font-semibold` |
| Body / answer | 15-16px | 400 | 1.6 | `text-base leading-relaxed` |
| Label | 14px | 500 | 1.4 | `text-sm font-medium` |
| Caption / meta | 12-13px | 400 | 1.4 | `text-xs` |
| Domain / mono meta | 11-12px | 500 | 1.3, uppercase, tracking-wide | `text-[11px] font-mono uppercase tracking-wide opacity-60` |

Weight ceiling: 600 (semibold) for headings and emphasis; body text stays
400. This is NOT a hard "never 700" brand rule (that was the old
Perplexity-only claim); see `patterns-typography.md` for per-product
divergence (Google/Gemini headings do use 700 in places).

## Spacing rhythm

Base unit 4px (Tailwind's default scale, daisyUI does not override it).

| Token | px | Tailwind | Use |
|---|---|---|---|
| xs | 4 | `gap-1` / `p-1` | icon-to-label gaps |
| sm | 8 | `gap-2` / `p-2` | chip/pill internal padding (vertical) |
| md | 12 | `gap-3` / `p-3` | card internal padding, row gaps |
| lg | 16 | `gap-4` / `p-4` | section internal padding |
| xl | 24 | `gap-6` / `p-6` | section-to-section rhythm |
| 2xl | 32-40 | `gap-8`/`gap-10` | landing hero block rhythm (wordmark -> tagline -> pill) |

## Radii

daisyUI 5 structural theme variables (names + example values from
daisyUI's own theme-authoring template; cauce has not overridden any of
these, so it runs on daisyUI's built-in defaults, not the numbers below,
which are shown as reference for what each variable governs):

| Variable | Governs | Typical cauce use |
|---|---|---|
| `--radius-selector` | checkbox, toggle, radio, badge | segmented-control track/thumb |
| `--radius-field` | button, input, select, tab | pill search input, chips |
| `--radius-box` | card, modal, alert | source cards, settings dialog |

Full pill shape (`rounded-full` / 9999px) is used explicitly for the
search input and mode segmented control regardless of `--radius-field`,
matching the cross-product convergence in `patterns-search-input.md`.

## Elevation / depth

daisyUI 5 ships a `--depth` variable (0 or 1) that adds a soft 3D bevel to
components like buttons; cauce has not overridden it (default applies).
Beyond that, shadow values observed converging across source-card hover
states in `patterns-motion.md`:

| Token | Value | Use |
|---|---|---|
| resting | `0 1px 2px rgba(0,0,0,0.05)` | source card at rest |
| hover-lift | `0 2px 8px rgba(0,0,0,0.08)` | source card hover |
| modal | `0 10px 15px -3px rgba(0,0,0,0.1), 0 4px 6px -2px rgba(0,0,0,0.05)` | settings dialog, popovers |

## Color roles

daisyUI semantic roles (from daisyUI docs) mapped to cauce usage. cauce uses
the built-in `light`/`dark` daisyUI theme pair unmodified:

| daisyUI role | cauce usage |
|---|---|
| `base-100` | page canvas |
| `base-200` / `base-300` | elevated surfaces (dropdown, source card background) |
| `base-content` | body text |
| `primary` | title-link accent, focus ring, active segmented-control state |
| `neutral` | muted chrome (borders, disabled state) |
| `info` | "searching" phase pill (see `patterns-motion.md` phase model) |
| `success` | "writing" phase pill / cache-hit badge |
| `warning` | rate-limit / retry state |
| `error` | backend error state |

cauce's legacy static-site CSS (`ui/src/index.css` `:root` custom
properties: `--accent #aa3bff`, `--text`, `--bg`, etc.) predates the
daisyUI migration and is a separate, non-daisyUI token set scoped to the
old landing markup; it is out of scope for the SPA screens this
`references/` directory documents and is called out here only so it is
not mistaken for the active token set.

## Motion durations

Canonical list lives in `patterns-motion.md`; repeated here as the numeric
source of truth:

| Token | ms |
|---|---|
| instant | 0 |
| fast | 120 |
| standard | 220 |
| slow | 360 |

daisyUI ships no motion-duration theme variable; these are plain
Tailwind/CSS values (`duration-[120ms]` etc.), not daisyUI tokens.

## Sources

- `ui/src/lib/theme.ts`, `ui/src/index.css` (cauce's actual theme wiring, read
  2026-09-18).
- https://daisyui.com/docs/colors/ (semantic color roles)
- https://daisyui.com/docs/themes/ (theme authoring template, structural
  variable names and example values)
- https://blog.logrocket.com/daisyui-5-whats-new/ (`--depth`, `--noise`
  introduction in daisyUI 5)
