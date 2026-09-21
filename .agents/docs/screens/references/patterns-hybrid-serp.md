# Pattern: Hybrid SERP (classic results + AI answer, one surface)

How classic result lists and AI answers coexist on one product across the products that
actually ship both, rather than modelling oxe on an answer-only product with no classic half.
This is the core gap the original five reference docs left open: they were researched almost
entirely against Perplexity, which has no classic SERP at all.

## Applies to

`search.md` Mockup A (Search mode) and Mockups B/B2 (AI mode) — checkpoints 3, 4, 9, 10, 11,
13, 14 in `userflow-checkpoints.md`. Governs the mode switch itself, and how oxe's two modes
share (or don't share) layout and navigation with each other.

## The mode-switch convention

- **Google AI Mode**: ships as a peer tab next to `All / Images / Videos / ...`, not an
  in-composer toggle. Selecting it navigates to a distinct AI-first results view; the classic
  tabs remain reachable via the same tab row. AI Overviews is different from AI Mode: it is an
  inline block injected at the top of the normal `All` results, opt-out-able per query, coexisting
  with organic results on the same page rather than replacing them.
- **DuckDuckGo**: the classic SERP gets an inline "Search Assist" AI block above organic
  results for qualifying queries (`shots/duckduckgo-serp-search-assist-dark.png` — the AI block
  sits above the ten blue links, visually distinct with its own card treatment, organic results
  continue below unmodified). Duck.ai is a fully separate product/URL, not a mode switch on the
  SERP at all — closer to Google's AI-Mode-as-distinct-surface than to Search Assist's inline
  block.
- **Brave**: Answer with AI renders as a block at the top of the results page for qualifying
  queries, similar in spirit to Google's AI Overviews (opt-out via settings, not a per-query
  toggle); organic results follow below.
- **Kagi**: Quick Answer is a small, compact block above results, deliberately terse (closer to
  a featured-snippet than a full AI answer surface) — the least visually dominant of the
  harvested set, reflecting Kagi's "answers should not crowd out links" positioning.
- **Bing Copilot**: closer to Google AI Mode's distinct-surface model — Copilot is reachable as
  its own tab/pane rather than injected inline into the classic results.

Two shapes recur: **inline block above organic results** (DuckDuckGo Search Assist, Brave
Answer with AI, Kagi Quick Answer — the AI content is a peer element on the same page as the
links) and **separate peer surface** (Google AI Mode, Bing Copilot, Duck.ai — the AI content
replaces the whole view, reachable by a tab or distinct entry point). oxe's own `mode=ai` on the
same route is closer to the second shape (a distinct surface) but reached via URL param rather
than a visible tab row — worth flagging as a divergence from both shapes: neither harvested
product switches modes via a value in the search-box's own query params.

## Opt-in vs default

- **Default-on, inline**: DuckDuckGo Search Assist, Brave Answer with AI, Kagi Quick Answer —
  these appear automatically for qualifying queries without the user asking for AI explicitly.
- **Opt-in, distinct surface**: Google AI Mode, Bing Copilot, Duck.ai — the user actively
  switches into these; the default experience is the classic SERP.
- **Always-on (no classic mode exists)**: Perplexity, Gemini — the answer-only extreme, kept in
  this directory only as a comparison point, not a target.
- oxe: `mode=ai` is opt-in (default is Search mode), which puts it in the second bucket, aligned
  with Google AI Mode / Bing Copilot rather than the always-on inline-block products.

## Placement relative to organic results

- Inline-block products (DuckDuckGo, Brave, Kagi) place the AI content ABOVE organic results,
  never below, never interleaved — a user scrolling past the AI block always reaches an
  unmodified classic list.
- Distinct-surface products (Google AI Mode) don't have this question at all: switching modes
  replaces the results view rather than augmenting it. Where sources exist, Google AI Mode
  attaches them as a dedicated rail rather than reusing the classic result-card layout (see
  `patterns-layout-grid.md`'s two-column note, `shots/google-aimode-answer-complete-twocolumn.png`).

## How a user gets back to plain links

- Inline-block products: trivial — organic results are already on the same page, below the AI
  block, at all times.
- Distinct-surface products: an explicit navigation action is required. Google AI Mode: switch
  tabs back to `All`. Bing Copilot: switch pane/tab back to Search. Duck.ai: navigate back to
  duckduckgo.com.
- oxe's contract (per `search.md` and `patterns-states.md`'s Hybrid section) gives both
  directions an explicit escape hatch even on failure states: `ask AI instead` from a classic
  zero-result or error state, `view Search` from an AI empty-sources or mid-stream-failure
  state. This is closer in spirit to the always-reachable link list of the inline-block products
  than to the tab-switch of the distinct-surface products, even though oxe's mode switch itself
  behaves like a distinct surface. That combination (distinct surface, but always-available
  lateral escape hatch on failure) has no single competitor precedent — it borrows the safety
  net of the inline-block products without their layout constraint of always showing links.

## Sources

- `shots/duckduckgo-serp-search-assist-dark.png` (inline AI block above organic results)
- `shots/google-aimode-answer-complete-twocolumn.png`, `shots/google-aimode-composer-dark-idle.png`,
  `shots/google-aimode-idle-example-prompts.png` (AI Mode as distinct surface)
- `shots/duckai-landing-idle.png`, `shots/duckai-landing-typed-idle.png`,
  `shots/duckai-answer-complete-privacy-banner.png` (Duck.ai as separate product)
- https://www.nngroup.com/articles/google-ai-mode/ (AI Mode vs AI Overviews distinction)
- `patterns-layout-grid.md` (column-width consequence of the two placement shapes)
- `patterns-states.md` (the Hybrid escape-hatch contract this doc's last section cites)
- `search.md` (oxe's own `mode=ai` contract)
