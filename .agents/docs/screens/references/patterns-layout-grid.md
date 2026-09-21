# Pattern: Layout grid

Page frame, header anchoring, and content column widths for a hybrid
product that renders both a classic result list and a centered AI answer
on the same route. The core fact this doc exists to state: **a classic
result list and an AI answer are not the same container.** A left-anchored
list of ten blue links and a centered prose column read wrong if forced
into one fixed-width shell; treating them as interchangeable is the most
common layout mistake when bolting an AI mode onto a classic SERP.

## Applies to

- `landing.md` (hero column, checkpoints 1-5 in `userflow-checkpoints.md`).
- `search.md` Mockup A (Search mode column) and Mockups B/B2 (AI mode
  column) — checkpoints 3, 4, 9, 10, 11, 13, 14.
- `history.md` (table container, checkpoints 15-17).
- `dashboard.md` (panel grid, checkpoint 20).

## Page frame

- Header is fixed-position or sticky-top across every screen (`oxe /
  search / history / dashboard` nav left, `(?) settings [gh] version`
  right), never scrolls out of view. Confirmed pattern across all
  harvested products: Google, DuckDuckGo, Brave, Kagi, Bing all keep the
  nav/tab row pinned above the results well.
- Full-bleed canvas background; content sits in one or two centered
  columns depending on mode, never a boxed card around the whole page.
- Footer: none on search-result screens in any harvested product; a
  disclaimer line ("may contain inaccuracies") sits directly under the
  answer instead of in a page footer (see `patterns-states.md`).

## Content column widths per mode

This is the gap oxe's prior docs had zero coverage of.

- **Classic result list (Search mode)**: left-anchored, narrower column,
  ~600-652px, NOT centered as a block of prose — DuckDuckGo, Google, and
  Bing all left-align the result column with the search box's left edge,
  leaving the right two-thirds of the viewport empty at desktop widths
  (`shots/duckduckgo-serp-search-assist-dark.png` shows the ~650px column
  against a ~1500px+ viewport). oxe's `search.md` Mockup A specifies
  "~652px content column, centered" — note this is centered as a *column*
  (the column itself sits mid-page) but its internal content (favicon,
  title, snippet) is left-aligned text, not centered text.
- **AI answer column**: also centered, similar or slightly wider
  measure (~680px / 68ch) for the prose itself, but Google AI Mode's
  completed-answer layout (`shots/google-aimode-answer-complete-twocolumn.png`)
  goes further: a **two-column split** once sources exist — answer prose
  in a left column (~600-650px) and a separate vertical "Sources" rail
  with rich cards (thumbnail, title, snippet, date) in a right column at
  desktop widths. oxe's spec keeps sources as a horizontal scroll row
  below the answer rather than a side rail (`search.md` Mockup B2); note
  this as an intentional oxe divergence, not a gap, since the horizontal
  row survives to narrow viewports without a breakpoint rewrite.
- Convergence: both modes' primary text column caps around 600-680px
  regardless of viewport; neither mode lets prose or result text run
  edge-to-edge, confirmed across Google, DuckDuckGo, Brave, Kagi.

## Header anchoring inside the column

- Landing: input pill is the sole focal element, vertically centered
  around 42-45% viewport height (`landing.md`), not vertically centered
  at 50% — every harvested landing (Google, DuckDuckGo duck.ai, Brave,
  Ecosia, Gemini) sits its input in the upper-middle third, leaving more
  space below for whatever will render there (results, first answer
  tokens) than above.
- Results: input pill moves to the top of the content column and stays
  there (results-header pill, `search.md`), never re-centers mid-page.

## Gutters and breakpoints

- Desktop ≥1024px: single content column (Search or AI) with generous
  side margins; nothing else competes for horizontal space except the AI
  Mode two-column sources rail described above.
- Tablet ~768-1023px: source-card row and AI two-column layouts collapse
  to a single stacked column; DDG and Google both drop side-by-side
  panels here.
- Mobile ~390-430px: full-width content column with ~16-24px side
  gutters; header nav collapses to icons/hidden labels (`oxe`'s header
  spec: "version label hides below ~640px").
- oxe-specific breakpoints already pinned in the screen specs: header
  input/toggle stack below ~700px (`search.md`), dashboard panel grid
  goes single-column below ~700px (`dashboard.md` `repeat(auto-fit,
  minmax(20rem, 1fr))`), history table's url column hides below ~768px
  (`history.md`).

## Fixed vs scrolls

- Fixed: header/nav row (all screens), the AI mode's bottom-pinned
  follow-up composer once an answer exists (Gemini and duck.ai both pin
  a reply composer to the viewport bottom, not the content-column
  bottom: `shots/gemini-answer-complete-pinned-composer.png`,
  `shots/duckai-answer-complete-privacy-banner.png`).
- Scrolls: the result list / answer column vertically; the source-card
  row horizontally (oxe) or the whole page vertically for Google AI
  Mode's two-column layout (no independent scroll region for the
  sources rail there).
- oxe's `search.md` follow-up input lives in the results-header pill at
  the TOP of the column, not bottom-pinned like Gemini/duck.ai; this is
  a deliberate divergence already pinned in the spec (one input, one
  location, shared between modes) and should not be "fixed" to match the
  chat-style bottom composer pattern without a spec change.

## Sources

- `shots/duckduckgo-serp-search-assist-dark.png` (classic column width,
  AI block above organic results, nav tab row pinned)
- `shots/google-aimode-answer-complete-twocolumn.png` (two-column answer
  + sources rail)
- `shots/gemini-answer-complete-pinned-composer.png`,
  `shots/duckai-answer-complete-privacy-banner.png` (bottom-pinned
  composer pattern, answer-only products)
- https://www.nngroup.com/articles/google-ai-mode/ (AI Mode as
  full-page vs AI Overviews as inline block)
- `.agents/docs/screens/landing.md`, `search.md`, `history.md`,
  `dashboard.md` (oxe's own column/breakpoint contract)
