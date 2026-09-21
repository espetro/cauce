# Reference docs: design pattern research for oxe

This directory holds researched, cited design-pattern references that back
the screen specs in `.agents/docs/screens/*.md`. It is research material,
not the spec itself: change the screen specs (`landing.md`, `search.md`,
`history.md`, `dashboard.md`) when behaviour changes; change these docs
when the underlying product research changes or widens.

## Index

- [`tokens.md`](tokens.md) - machine-readable token set (type scale,
  spacing, radii, elevation, colour roles, motion durations), each mapped
  to its daisyUI 5 / Tailwind 4 equivalent. The other docs link here
  instead of restating pixel values.
- [`patterns-search-input.md`](patterns-search-input.md) - input
  placement lifecycle, sizing, placeholder copy, mode toggles, keyboard
  affordances, focus state; includes a `## Hybrid (classic + AI)` section
  on the same input serving both modes.
- [`patterns-answer-streaming.md`](patterns-answer-streaming.md) - AI
  answer phase model, order of reveal, streaming contract, citations.
- [`patterns-typography.md`](patterns-typography.md) - type scale,
  researched across 4+ products with per-product divergences noted
  separately from the converged "general" scale.
- [`patterns-states.md`](patterns-states.md) - empty, loading, error,
  rate-limit, stopped states; includes a `## Hybrid (classic + AI)` section
  distinguishing the classic SERP's plain "no results" from the AI answer's
  richer failure states.
- [`patterns-motion.md`](patterns-motion.md) - duration tokens, signature
  motions, reduced-motion behaviour.
- [`patterns-hybrid-serp.md`](patterns-hybrid-serp.md) - how classic
  result lists and AI answers coexist on one surface across products:
  Google AI Mode / AI Overviews, DuckDuckGo Search Assist vs Duck.ai,
  Brave Answer with AI, Kagi Quick Answer, Bing Copilot. Opt-in vs default,
  placement relative to organic results, how a user gets back to plain
  links.
- [`patterns-layout-grid.md`](patterns-layout-grid.md) - page frame,
  header anchoring, content column widths per mode, gutters, breakpoints,
  fixed vs scrolling regions.
- [`patterns-result-list.md`](patterns-result-list.md) - classic SERP
  anatomy: favicon + domain line, title link, snippet clamp, metadata row,
  vertical rhythm between results.

## Per-product insights

[`insights/`](insights/README.md) holds one deep-dive per product (Google,
DuckDuckGo, Brave, Gemini, SearXNG), each ending in a `## Verdict for oxe`
list. These are the evidence layer behind the `patterns-*.md` docs, not a
second rule set: a Verdict item only becomes a rule once a second product
converges on it and it is promoted into the matching pattern doc, or it is
adopted deliberately into a screen spec. The evaluator in the UI loop is
handed the pattern docs and `tokens.md`, not `insights/`, so a single
product's choice cannot be graded as if it were a convention.

## Capture method

Two sources of screenshots, both under `shots/`:

1. **Competitor captures**: manual screenshots of live products (Google,
   Google AI Mode, DuckDuckGo classic + Search Assist + Duck.ai, Gemini,
   Brave, Ecosia), taken by the user during design research, some
   annotated with red boxes calling out the specific region under
   discussion (e.g. `gemini-answer-complete-pinned-composer.png`,
   `duckduckgo-serp-search-assist-dark.png`). File names describe product,
   surface, and state (`<product>-<surface>-<state>.png`); annotation
   boxes are incidental, not part of the naming.
2. **oxe app captures**: screenshots of oxe's own running app, used to
   document a real bug or in-progress state rather than a competitor
   pattern (e.g. `oxe-app-ai-answer-streaming-jsonleak-bug.png`, which
   caught the raw JSON metadata leak fixed by the
   `fix(ai): stream tail-stripped answer in delta frames` commit). These
   are evidence, not pattern citations: do not cite an oxe screenshot as
   proof of a competitor convention.

No automated capture pipeline exists yet (no Playwright harness pointed at
competitor sites); all screenshots here are manual. If a future update
adds automated capture, record the method here.

## The `## Applies to` convention

Every `patterns-*.md` file has an `## Applies to` section naming which
oxe checkpoints (from `userflow-checkpoints.md`) or screens (from
`landing.md` / `search.md` / `history.md` / `dashboard.md`) the doc
governs. This is how a pattern doc stays a reference rather than becoming
generic design trivia: if you can't name a checkpoint it applies to, the
content probably belongs in a note or a different doc, not here.

## The patterns-not-targets rule

**Never derive a whole scale, rule, or constraint from a single product's
implementation.** A pattern needs convergence across 2+ products to be
documented as a general rule; one source is an anecdote, not a reference.
Perplexity-only observations (or Gemini-only, or any single product) go in
a clearly labelled per-product divergence note, not into the "general"
row of a scale or the top-level bullet of a rule. This rule exists because
the original five docs in this directory skewed almost entirely to
Perplexity's own brand choices (e.g. "font-weight never above 500") stated
as if they were universal AI-search conventions; de-skewing that mistake
is why this README and the two newest docs (`patterns-hybrid-serp.md`,
`patterns-layout-grid.md`) exist.
