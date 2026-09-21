# Per-product insights: competitor UI/UX deep dives

Deep per-product analyses backing the pattern docs in the parent directory
(`../patterns-*.md`) and the screen specs in `../../`. The pattern docs
extract converged rules (2+ products); these docs hold the per-product
detail, divergence notes, and evidence that the extraction drew from.
The `../README.md` patterns-not-targets rule applies here too: a single
product's choice documented below is an anecdote, not a rule.

## Index

- [`google.md`](google.md) - Google Search + AI Mode / AI Overviews.
  In-composer AI toggle on every surface, inline favicon citation chips
  with overflow counter, two-column answer + Sources rail, branded
  shimmer thinking phase, silent low-confidence fallback to links.
- [`duckduckgo.md`](duckduckgo.md) - DuckDuckGo classic + Search Assist +
  Duck.ai. Motion restraint as a deliberate choice, query-gated inline AI
  card, privacy chrome ("Anonymized by...") placement, weak citations
  oxe can out-do. Doubly relevant: oxe's backend IS DDG, so the doc also
  covers backend-driven UX (snippet bold terms, favicon fallback,
  throttle/CAPTCHA states).
- [`brave.md`](brave.md) - Brave Search classic + AI Answers + Ask Brave +
  Goggles. Offset-positioned citation events from their Answers API,
  two-tier AI escalation (inline block then chat surface), decoupled
  AI-block loading, Goggles custom-ranking grammar as precedent for
  oxe's cache-transparency panel.
- [`gemini.md`](gemini.md) - Gemini (answer-only extreme, comparison
  point not target). Composer morph transition, collapsible streamed
  "Thoughts", web-grounding query chips + per-claim citations,
  bottom-pinned composer (oxe deliberately diverges).
- [`searxng.md`](searxng.md) - SearXNG as instantiated on public
  instances. Server-rendered no-JS-first restraint, per-result engine
  chips (metadata transparency unique in the set, compared directly
  against oxe's cache meta line), partial-failure aggregation states,
  ecosystem patterns for bolting AI answers onto a metasearch backend.

- [`dispositions.md`](dispositions.md) - one disposition (covered, promoted,
  queued, dropped) for every verdict item across the five docs. Evaluator-blind.

## Evidence base

Screenshots in `../shots/` (google-*, duckduckgo-*, duckai-*, gemini-*,
brave-landing-idle) are primary evidence for four of the five; SearXNG
has no shots (antibot walls on live instances) and was researched from
repo sources and docs instead, noted in its Sources section. Each doc
lists its web sources; fetch dates are the doc's commit date.

## Method

Five parallel research passes, one per product, each grounded in the
local shots plus web research, each constrained to the same five-part
skeleton (pages/surfaces, state transitions, motion, AI retrieval and
source richness, layout/design system) plus the repo-standard
`## Applies to`, `## Sources`, and `## Verdict for oxe` sections.
