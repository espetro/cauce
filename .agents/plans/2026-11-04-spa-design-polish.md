# SPA design polish — DS-20 (post-bits-ui craft pass)

Status: in progress. Branch `v3/ds-polish` (off `main` @ d711e37).
Scope: `crates/cauce-server/web/src/spa/` — visual craft only, no wire
changes, no new dependencies.

Parent: `2026-10-09-bits-ui-migration.md` (the DS system landed in PR
#291 — bits-ui wrappers, tokens, no raw widgets left). This plan is the
*craft* layer the DS plan never specified: the plan set the components
and the tokens, but left typography, elevation and spacing rhythm
undefined, so every surface renders at browser-default 16px.

Audit that drives this: `.agents/notes/2026-11-04-spa-ux-audit.md`.

## 1. Settled decisions

1. **Bits UI stays.** No daisyUI, no shadcn-svelte. The `ui/` wrapper
   layer IS the design system; this plan builds on it, not beside it.
2. **One type scale, in tokens.** `--font-*` sizes and `--text-*` line
   heights live in `app.css` `@theme inline` beside the colour tokens;
   `h1`–`h4` get one base rule each. Components stop hand-rolling
   font-size (the dashboard's `0.9rem` h2 override is deleted).
3. **Typeface.** Keep the system stack — but the app loads a
   self-hosted variable font? NO: a webfont costs a round trip and the
   plan's "quiet reading room" reads fine on the system stack. Instead
   the craft comes from *scale, weight, tracking and measure*, not
   from a novelty face. (Deliberate: no Google Fonts request, no
   third-party font CDN, no FOUT.)
4. **Elevation.** One shadow recipe (`--shadow`) for floating layers
   only. Static surfaces get hairlines (already true). Panels get a
   second, weaker ambient recipe `--shadow-sm` for the dashboard
   cards, so the card grid reads as a set instead of a table.
5. **Control height.** One token `--control-h: 2rem`; every control
   (input, select, button, toggle) uses it. Filter bars and form rows
   stop having three different heights.
6. **The search page shows the query once.** The `prompt-card` is an
   answer-thread affordance; `/app/search` does not use it (the meta
   line already carries the count + request id). `/app/answer` keeps it.
7. **No landing-page hero rewrite.** The landing stays a single
   composer; the page gains an eyebrow + one-line subtitle so a
   first-time visitor can name the product in five seconds.

## 2. Token additions (DS-00 layer, `app/app.css`)

- `--font-sans` — the system stack, as a token (so `body` and the
  `.md` answer tree share one value).
- `--font-display` — same family, for headings only (the token exists
  so a future face swap is one edit).
- `--text-xs/sm/base/lg/xl/2xl` — the size scale (0.75 / 0.875 / 1 /
  1.125 / 1.375 / 1.75rem).
- `--leading-tight/normal/relaxed` — 1.2 / 1.5 / 1.65.
- `--tracking-tight/normal/wide` — -0.011em / 0 / 0.08em.
- `--control-h` — 2rem (32px).
- `--shadow-sm` — `0 1px 2px rgb(0 0 0 / 0.05)` (panels only).
- `--surface` — the raised surface colour for cards (light: `#ffffff`
  on `#fbfbfc` page? NO — keep `--bg` as the page and add `--surface`
  = same `--bg`; the panel reads through the hairline + shadow, not a
  second colour).

## 3. Tasks

### DS-20A — type scale + base headings (`app/app.css`)
Tokens + `h1`–`h4` base rules + `.page-title` / `.eyebrow` utility
classes + body font/measure/leading. Delete the per-component h2
overrides (dashboard `text-transform: lowercase`).

### DS-20B — search page: one query, not two
`routes/SearchPage.svelte` — drop the `prompt` prop (search meta line
already carries the context). `app/ConversationShell.svelte` — the
prompt card stays for the answer route.

### DS-20C — landing hero copy
`features/landing/Landing.svelte` + `routes/HomePage.svelte` — eyebrow
("metasearch for humans and agents") + one-line subtitle; the `vh` h1
stays for axe, a visible `<p class="eyebrow">` and subtitle carry the
message.

### DS-20D — control heights + filter bar
`app/app.css` — `--control-h` applied to `.filters input`, `.settings-form
.frow input`, `.ui-select-trigger`, `.ui-button[data-size="md"]`,
`.ui-toggle-group` item. One radius token for all.

### DS-20E — archive/dashboard row grid
`features/archive/ArchiveView.svelte` + `app/app.css` — the row becomes
a grid (`chevron | title+host | date | action`) so wrapped titles don't
drop under the chevron.

### DS-20F — panel craft
`features/dashboard/DashboardView.svelte` + `app.css` — panels get
`--shadow-sm`, `--surface`, consistent padding (0.875rem), and the h2
uses the shared scale.

### DS-20G — sweep
Zero raw widgets (already true), rebuild `assets/spa/`, run the four
gates, commit.

## 4. Gates

`pnpm run check:spa`, `pnpm run typecheck`, `pnpm run build:spa`,
`pnpm run test`, then `mise run validate` at the repo root.

## 5. Out of scope

- Landing page full redesign (a marketing page is a separate plan).
- Webfonts.
- The residual SSR pages (`/trace/{id}`, `/answer/{id}`) — they mirror
  the tokens in `src/pages/mod.rs`; only touch if a new token lands
  there.
