# Pattern: Search Input UX

How leading AI search sites design the query input.

## Applies to

Checkpoints 1, 2, 3, 5, 6 in `userflow-checkpoints.md` (landing idle,
landing AI mode, search submit, search loading, suggestions dropdown).
Governs `landing.md`'s hero pill and mode toggle, and the results-header
pill shared by `search.md`.

## Placement lifecycle

- Landing: input is the hero, centered at ~38-42% viewport height, max-width 640-680px.
- Results: query echoes at top of page (Google AI Mode lists the query at top and moves the input to a chat-style "Ask a follow up..." field pinned at the bottom). Perplexity keeps a compact pill at the bottom of the answer column.
- Rule for oxe: two anchor points. Hero input pre-search; compact sticky follow-up input below the answer once results render. Never scroll the input out of view (smoothui.dev "input always visible" contract).

## Sizing and shape

- Height 48-56px desktop (Perplexity composer: padding 14px 20px, 16px font). Touch target min 44px.
- Pill radius 9999px is the dominant identity shape (Perplexity, You.com). Some use 12px rounded rect (ChatGPT composer). Pick one; Perplexity treats the pill as non-negotiable brand.
- Max text measure ~65-75ch; composer wider than answer column is fine but keep within 720px.
- Multi-line growth: single row at rest, grows to ~5 rows (160px) max, then internal scroll.

## Placeholder text

- Conversational, verb-led: "Ask anything..." (Perplexity), "Ask a follow up..." (Google AI Mode). Avoid "Search..." for AI modes; it signals keyword matching.
- Placeholder color muted: ~#7a7367 on cream, base-content/50 equivalent in daisyUI.

## Mode toggles and attach controls

- Mode switch lives INSIDE the composer, not as a page-level tab where possible: model picker / mode chips at the input's left or bottom-left, small (13-14px, weight 500) pills.
- Google ships an explicit "AI Mode" tab next to All/Images/Videos filters; predictability of mode mattered more than the toggle itself in their research.
- Action icons right-aligned inside input: submit arrow (disabled at 40% opacity until text), stop button REPLACES submit in the same slot during streaming.

## Keyboard affordances

- Enter submits, Shift+Enter newlines. Show a subtle kbd hint ("↵") inside the composer on desktop only.
- "/" focuses the input from anywhere on the page.
- Disable submit (not the whole input) while a stream is active; keep Stop always reachable.

## Focus state

- Focus ring: 2px accent ring, ~40% alpha (Perplexity: `0 0 0 2px rgba(32,128,141,0.40)`), border shifts to accent. Transition 120-150ms ease-out.

## Hybrid (classic + AI)

- oxe runs one input for both modes, not two separate composers: the same
  `<SearchBox>` renders on `landing.md` and on the results-header pill in
  `search.md`, with a segmented Search/AI toggle built into the pill
  itself rather than a page-level tab. This is oxe's own decision, closer
  to duck.ai's and the oxe-mock composer screenshots
  (`shots/oxe-mock-pill-light-segmented-idle.png`,
  `shots/oxe-mock-pill-dark-segmented-idle.png`) than to Google's
  AI Mode, which ships the mode switch as a peer nav tab next to
  All/Images/Videos, not inside the composer (see
  `patterns-hybrid-serp.md` for the full cross-product comparison).
- Submitting the same query in the two modes produces genuinely different
  layouts, not just different content in the same shell:
  Search mode returns a result list (`patterns-result-list.md`); AI mode
  streams an answer (`patterns-answer-streaming.md`). The toggle re-runs
  the identical query string in the other mode and rewrites the url
  (`&mode=ai` added/stripped) rather than opening a new composer, so
  switching modes never clears what the user typed.
- Placeholder text swaps with mode (`Search privately` vs
  `Ask anything privately`, per `landing.md`), matching the pattern this
  doc already documents (verb-led placeholder signals the mode).
- When AI is unavailable, the AI segment stays visible but disabled
  (dimmed, tooltip) in both the landing pill and the results-header pill;
  it never hides and landing falls back to Search mode. This differs from
  every harvested competitor except Google, none of which need an
  "AI unavailable" state because their AI surface is either always-on
  (Search Assist) or a fully separate product (Duck.ai) rather than a
  conditionally-available mode of the same composer.
- Suggestions dropdown (`landing.md` State 5) is shared between modes and
  between the landing and results-header inputs: same component, same
  keyboard map, regardless of which mode is active when the user starts
  typing.

## Sources

- https://blakecrosley.com/guides/design/perplexity
- https://www.webdesignhot.com/design.md/perplexity/
- https://blog.google/products-and-platforms/products/search/ai-mode-development/
- https://9to5google.com/2025/03/05/google-search-ai-mode-announcement/
- https://skills.smoothui.dev/docs/ai-chat
- `landing.md`, `search.md` (oxe's own hybrid input contract)
