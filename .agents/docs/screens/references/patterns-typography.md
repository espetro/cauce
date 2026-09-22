# Pattern: Typography Hierarchy

Question vs answer vs citation text across AI search products.

## Applies to

Checkpoints 1, 3, 9, 10 in `userflow-checkpoints.md` (landing hero type,
classic result title/snippet, AI answer body, query echo). Governs
`landing.md`'s wordmark/tagline, `search.md`'s result titles/snippets and
AI answer body/headings.

## Scale (converged across Google AI Mode, DuckDuckGo, Brave, Kagi, Perplexity)

| Role | Size | Weight | Line height | Tracking | Use |
|---|---|---|---|---|---|
| Display hero | 40-56px (2.5-3.5rem) | 500-600 | 1.1 | -0.02em | Landing hero only |
| Heading large | 24-28px | 600 | 1.3 | -0.01em | Thread title / echoed query |
| Heading | 20-22px | 600 | 1.35 | -0.01em | Answer section headers |
| Answer body | 15-16px | 400 | 1.6 | normal | THE hero: streamed answer |
| Body | 15px | 400 | 1.6 | normal | UI descriptions |
| Label | 14px | 500 | 1.4 | normal | Buttons, chips, tabs |
| Caption | 12-13px | 400 | 1.4 | normal | Metadata, timestamps |
| Source domain | 11-12px | 500 | 1.3 | 0.5px, uppercase, mono | Domain on source cards |
| Citation token | 12px | 500 | inherit | normal | Inline [1] refs |

This matches the canonical scale in `tokens.md` — that file wins if the two ever drift.

## Rules

- Answer body: 15-16px/1.6 minimum, max measure ~68ch (680px). The generous leading is the single most-cited typographic decision across the harvested set (magazine column, not form field).
- Question/echoed query is LARGER than the answer (20-28px, weight 600): the user's words are the page title. Confirmed on Perplexity, Google AI Mode, and Kagi.
- Negative tracking (-0.01 to -0.02em) on headings only; body and small text stay at normal tracking.
- Citations are typographic, not chrome: accent-colored superscript tokens in the text flow, baseline-aligned with body figures (Perplexity, Bing Copilot).
- Warm near-black ink (e.g. #091717 / #1a1a1b) rather than pure #000, common but not universal (Google keeps closer to true near-black).
- Weight ceiling for headings/emphasis: 600 (semibold). Body text stays 400. Never thin or black weights.
- Tabular numerals (`tnum`) on any counters/timestamps.

## Per-product divergence

Do not fold these into the "general" rows above — single-source observations, not convergent patterns:

- **Perplexity only**: caps weight at 500 everywhere (no 600/700 at all, including headings) — this is Perplexity's own brand constraint, not a cross-product convention. The general scale above uses 600 for headings because Google AI Mode, Kagi, and Bing Copilot all use 600+ weight headings.
- **Google / Gemini**: headings occasionally reach 700 (bolder than the 600 ceiling most other products use), and system font stack is less consistently used (Google Sans in places) versus the system-font convergence seen elsewhere.
- **System font stack**: acceptable and common (DuckDuckGo, Kagi, Bing all lean on system fonts; Perplexity uses a custom FK Grotesk/pplxSans but its own analyses note neutrality was the design goal, not a special typeface being load-bearing). Pair with a mono stack for domains/code: ui-monospace, SFMono-Regular, Menlo.

## daisyUI mapping for cauce

- Answer body: `text-base` (16px), `leading-relaxed` (1.625), max-w-prose (65ch).
- Query echo: `text-xl md:text-2xl font-semibold tracking-tight`.
- Chips/labels: `text-sm font-medium`.
- Source cards: title `text-sm`, domain `text-[11px] font-mono uppercase tracking-wide opacity-60`.

## Sources

- https://unpkg.com/oh-my-design-cli@1.9.0/web/references/perplexity/DESIGN.md
- https://www.webdesignhot.com/design.md/perplexity/
- https://blakecrosley.com/guides/design/perplexity
- https://www.nngroup.com/articles/google-ai-mode/ (Google AI Mode heading/body treatment)
- `shots/duckduckgo-serp-search-assist-dark.png`, `shots/google-aimode-answer-complete-twocolumn.png` (heading weight and size observed directly)
- `tokens.md` (canonical numeric scale, cross-referenced above)
