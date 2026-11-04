# UX audit — cauce SPA (`main` @ d711e37, local serve on replay)

Date: 2026-11-04. Auditor: Maki (playwright-cli, axe-core 4.10.2).
Surfaces: /app (landing), /app/search?q=, /app/history, /app/settings,
/app/dashboard, /app/archive, /app/answer. Viewports 1440×900 + 375×812,
light + dark themes.

## Verdict block

```
VERDICT: Fail (High findings present; hard gates otherwise green)

Persona: self-hosting developer triaging their own instance
         (+ first-time-user lens on every surface)
Interaction Manifest: complete (search typed+submitted, settings loaded,
  dropdown opened, theme cycled, mobile sweep, axe per route)
Hard gates: console errors 0, warnings 0, network 5xx 0, 403/404 auth 0,
  layout-collapse 0 (375/768/1440 clean), axe Critical 0, axe Serious 0
Performance: search route hydrates < 1s, results paint < 400ms — green
Findings: Critical 0 · High 3 · Medium 6 · Low 4
```

## Findings

### H1 — Typography scale is absent (Visual, High)
No `h1`/`h2` rules anywhere in `app/app.css` (verified by grep and
computed style). Every `<h1>` renders 16px/400 — identical to body text.
Page titles ("Settings", "History", "dashboard") are visually
indistinguishable from paragraphs; dashboard panel `h2`s rely on a
per-component 0.9rem override that no other surface gets. The plan's
"aesthetic north star" (quiet reading room, Linear-grade density) is
unreachable without a type scale.

Reproduce: open /app/settings — the title is 16px regular.
Fix (smallest): token-based type scale in app.css `@theme` + base rules
for h1–h4 (h1 1.375/600 -0.01em, h2 1.125/600, h3 1rem/600); page-title
section-label eyebrow where the copy is quiet by design.

### H2 — /app/search renders the query twice (Interaction, High)
`ConversationShell.prompt-card` shows the submitted query, and the
compact Omnibox below it *also* holds the same text (bound to `page.q`,
which is initialized from the route param). Two identical "rust" boxes
stacked (screenshot audit-search-results.png). First-time users read it
as "did my search double-fire?".

Reproduce: /app/search?q=rust → prompt-card "rust" + composer value "rust".
Fix (smallest): `SearchPage` passes `prompt=""` (the card is an
answer-thread affordance; search has the meta line already) OR clears the
composer to placeholder. Choose: pass the prompt only in answer mode;
search keeps composer + meta.

### H3 — The landing hero omits the product's own name and value (First-time-user, High)
/app is a bare search box + "One server, three surfaces" below the fold.
No tagline, no statement of what cauce *is*. The LANDING flag exists but
the hero copy is hidden behind the `vh` class (the h1 is literally
`class="vh"`). A first-time visitor cannot answer "what is this?" in 5s.

Fix (smallest): visible eyebrow (metasearch for humans and agents) +
one-line sub-copy above the composer on /app; keep the quiet aesthetic.

### M1 — Duplicate `more pages` button in the a11y tree (Interaction, Medium)
Two `[aria-label="more pages"]` buttons exist in the DOM (one visible,
one `width=0` portal seed). Tab order shows it twice. Harmless visually,
noise for AT users.

### M2 — Dashboard `h2`s force `text-transform: lowercase` (Visual, Medium)
Panel titles render as "cache hit rate", "client split" — the i18n copy
is Title Case in en.yaml. Mixed signal: heading is quiet-grey lowercase
while the card next to it shouts `5%` at 1.6rem/700. Pick one voice.

### M3 — Archive rows have a hanging chevron + wrapped title (Visual, Medium)
Long titles wrap under the collapsed chevron (screenshot dark-dashboard:
"Introducing Clef…" title drops below the row baseline, chevron floats
alone). Grid the row: chevron | title | host | date | delete.

### M4 — `.filters` bar mixes select/checkbox/button/input with 3 different heights (Visual, Medium)
History filter row: UiSelect (28px), raw input (28px but different
padding), UiCheckbox + label, UiButton — baseline-aligned but visually
inconsistent radii/padding. Unify on one control height token.

### M5 — Result rows' favicon-title-host cluster has no optical alignment (Visual, Medium)
Host sits baseline-adjacent to the title with a 0.5rem gap but no
vertical alignment; favicons 16px sit above the x-line. Snippet + host
reading rhythm is fine; the first line is crowded.

### M6 — Settings BYOK section has no visual separation from admin form (Information scent, Medium)
For an admin the two fieldsets blur together; BYOK (browser-local) vs
instance config (server) deserve different container treatments (the
BYOK box could carry the `--greyed-bg` tint + a "stored in this browser"
chip).

### L1 — `page.q` not cleared after navigating to a new query from landing
Low impact: composer keeps previous query on the landing route after
back-nav. Cosmetic.
### L2 — Focus ring on prompt-card-less composer relies on border color only (contrast AA passes, but subtle)
### L3 — `.legend` chart swatches `i.c`/`i.n` are 10px squares — fine, but below the 24px non-text contrast target for UI components; acceptable as decorative.
### L4 — History `delete` buttons are `danger` red — correct — but the row also shows `ask again` accent link at the same weight; danger could be ghost+warn text to quiet the wall of red on long lists.

## Hard-gate scorecard
All zero; axe clean on all 6 routes (wcag2a/2aa/21a/21aa).

## TOP 5 (impact × ease)
1. H1 type scale — one CSS block, changes every page's perceived quality.
2. H2 double composer — 1-line prop change, removes the biggest "is this broken?" signal.
3. H3 landing hero copy — small, first-touch credibility.
4. M3 archive row grid — visible breakage on a primary surface.
5. M4 filter-bar control heights — one token, four surfaces.

## Hold this in your hands
The app feels like a well-made tool that someone built quickly and never
sanded: every primitive works, nothing is broken, but nothing tells you
it was *designed* — no type hierarchy, no elevation, uniform 16px
everything. It reads as "engineering UI" rather than "quiet reading
room". The bones (tokens, bits-ui wrappers, hairline aesthetic) are
genuinely good; what's missing is the last 10% of typographic and
spacing craft.
