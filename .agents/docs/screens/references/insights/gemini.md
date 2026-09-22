# Insight: Google Gemini (answer-only extreme)

Single-product insight doc. Per the README's patterns-not-targets rule,
everything here is a comparison point, not a target: Gemini is the far end of
the answer-only spectrum (no classic SERP exists anywhere in the product), so
it informs cauce's AI mode and landing screen, and is explicitly NOT the model
for cauce's search mode or its shared-input hybrid.

## Applies to

- `landing.md` (hero pill, personal greeting, composer anatomy)
- `search.md` Mockups B/B2 (streaming contract, grounding chips, sources
  treatment, pinned follow-up composer contrast)
- `references/patterns-layout-grid.md` (bottom-pinned composer evidence)
- `references/patterns-answer-streaming.md` (thinking phase contrast)

## 1. Surfaces

### Landing / hero (shots/gemini-landing-idle.png)

- No results page exists at all; the landing IS the app. Screen is a soft
  light-to-blue vertical gradient (off-white top fading to pale blue toward
  the pill), with nothing else on canvas.
- A large personalized greeting ("Hola, Quino. Què et passa pel cap?",
  cursive-adjacent Google Sans, roughly 34-40px, near-black ink on light)
  sits directly above the input pill. This replaces a wordmark/logo as the
  hero element: the identity of the page is a sentence addressed to the user,
  and it rotates per visit (time-of-day and content-aware variants).
- Composer pill sits directly under the greeting at roughly 38-45% viewport
  height, consistent with the upper-middle-third rule in
  `patterns-layout-grid.md`. Pill is near-full-content-width (wide, ~900px+
  class), fully rounded (9999px), white surface, subtle shadow.
- Composer anatomy, left to right: `+` (attachment/upload menu), large text
  placeholder ("Demana a Gemini" / "Ask Gemini"), right cluster holding a
  model picker ("Flash" + chevron, inline text button not a chip) and a mic
  icon. No mode toggle inside the pill (there is only one mode), no separate
  submit arrow visible while empty (enter submits).
- No suggestion chips, no recent-chats on the landing canvas itself. The
  empty state is deliberately minimal: greeting + pill. This is calmer than
  ChatGPT/duck.ai which seed starter prompts (see `patterns-states.md`).
- Sidebar: a left rail with "New chat" plus a scrollable list of recent
  chats; collapsed to an icon rail / hidden on smaller screens. History lives
  in the rail, not the landing canvas, so returning users resume from the
  rail while the canvas stays clean.

### Conversation thread

- Single centered prose column, roughly 700-768px max width, left-aligned
  text inside it. User turns render as compact rounded grey bubbles aligned
  right-ish within the column; Gemini turns render as plain full-width prose
  (no card, no avatar chrome in current 3.x-era UI).
- After the answer, a small action row: regenerate (circular arrow), copy,
  overflow (`...`) for thumbs/zoom variants. Actions are muted icons, ~20px,
  appearing on the completed turn.
- A disclaimer line sits under the pinned composer ("Gemini és una IA i pot
  cometre errors...", i.e. "Gemini makes mistakes, check info"), not in a
  page footer. Matches `patterns-layout-grid.md`'s no-footer rule.
- Model picker is per-turn: the "Flash" selector in the composer lets you
  switch models mid-conversation; thinking models expose their own surfaces
  (below).

### Canvas / Deep Research / temp chat (brief)

- Canvas: a second mode entered from the composer/tools menu where the
  thread pairs with an editable document panel beside the chat; the answer
  becomes an artifact you edit inline rather than streamed prose.
- Deep Research: choosing it turns the composer submission into a two-phase
  flow: Gemini first produces an editable research plan (user can "Edit
  plan"), then researches for minutes. During work the thread shows a
  "Researching..." status with "Show thinking" (streamed reasoning steps)
  and "Sites browsed" (clickable list of visited sites) as expandable
  affordances; a completion notification arrives if you leave the chat.
  The report lands as a long document with its own export actions.
- Temp chat / Gems / video tools are additional composer-adjacent entry
  points, confirming the pattern: every major capability is reached through
  the composer's tool cluster, not through top navigation.

## 2. State transitions

- Idle: greeting + pill, composer focused on load.
- Submitted -> thinking: with thinking models, a collapsible "Thoughts" /
  "Show thinking" region appears first. The reasoning streams inside it
  (lighter, muted) while a header row with a shimmering label indicates
  work; the region can be collapsed while it keeps streaming. This is the
  answer-only analogue of Perplexity's phase pill (`patterns-answer-streaming.md`),
  except the phase content is the model's actual reasoning, not a status label.
- Streaming: answer streams token-by-token below the thinking block; a
  trailing cursor tracks the last token.
- Complete: cursor drops, action row (regenerate/copy/overflow) fades in,
  follow-up suggestions may render as chips under the turn, and the
  bottom-pinned composer is fully active for the next turn.
- Grounded turns insert their own step: a "Searched for ..." / web-grounding
  chip group appears between the prompt and the answer (see section 4).
- Edit/resubmit: clicking a prior user bubble turns it into an editable
  text field; editing forks or rewrites the thread from that point. The
  composer also offers drafts/version affordances via the overflow menu on
  prior turns.
- Stop: a stop control replaces submission during generation; stopping keeps
  the partial text with a "stopped" indicator, matching the streaming
  contract in `patterns-answer-streaming.md`.
- Failure states are rare and generic (a prose apology with a retry), with
  none of the hybrid-mode asymmetry cauce needs (`patterns-states.md` ## Hybrid).

## 3. Motion

- Material 3 expressive baseline: durations in the 150-400ms band,
  emphasized easing curves, morphing shapes rather than fades for
  container-to-container transitions. The signature move is the composer
  morph: on first submit, the hero pill animates from mid-page to the
  viewport-bottom pinned position while the thread unfurls above it. It is
  one continuous transition, not a teardown/rebuild.
- Thinking shimmer: the "Show thinking" header text shimmers (gradient sweep
  across the label) while reasoning streams; the collapse/expand of the
  thoughts region animates height (~200-300ms) with a chevron rotation.
- Streaming cursor: small trailing caret at the write head; blink cadence
  near the ~530ms standard noted in `patterns-motion.md`. Auto-scroll tracks
  the tail and yields when the user scrolls up.
- Chip entry: grounding chips and follow-up suggestions fade/scale in
  quickly (~150-200ms) once available, after the answer completes.
- Reduced motion: thinking region appears expanded instantly, shimmer and
  cursor blink stop; consistent with `patterns-motion.md` ## Reduced motion.
- Overall language is cinematic and brand-forward (the gradient, the
  morphing pill); the rest of the thread is deliberately quiet so the motion
  budget is spent on the two identity moments (greeting, composer morph).

## 4. Web grounding and retrieval richness

- In the app, grounding is largely model-initiated: a row of pill chips
  ("Searched for: <queries>") appears above the answer, each chip opening a
  Google SERP panel for that query. Multiple fan-out queries may appear,
  matching the query fan-out behavior documented for the API
  (`webSearchQueries` in groundingMetadata).
- Inline citations are small clickable chips/numbers attached to the exact
  claim spans (the API grounds per text segment via groundingSupports, and
  the app renders that mapping). Hovering shows a source preview; clicking
  opens the source.
- After the answer, a horizontal source strip renders top sites (favicon +
  title), with a "Show all"/sources expansion for the full list; for Deep
  Research the richer "Sites browsed" list is exposed during the run.
- The API/ToS additionally requires Search Suggestions chips
  (`searchEntryPoint.renderedContent`, related-search pills bridging back to
  google.com) to be rendered unmodified with grounded responses. So the full
  Gemini grounding surface is: query chips above the answer, inline span
  citations in it, source strip below, related-search pills at the end.
- Compared with AI-search products (Perplexity, Google AI Mode in
  `patterns-hybrid-serp.md`): Gemini's grounding is opt-out-adjacent and
  app-consistent (chips always in the same place relative to the answer),
  but it is sparser than Perplexity's always-on numbered cards and lacks AI
  Mode's two-column sources rail. Because Gemini has no classic SERP, its
  grounding chips must double as "see the raw results" escape hatch, which
  is exactly the job cauce splits across the `view Search` hatch and the
  source-card row.

## 5. Layout and design system

- Full-width conversational column: one centered column (~700-768px) for
  the thread; no left content gutter, no right rail on the main surface.
  Sidebar rail (chats, Gems, settings) is the only permanent frame element,
  and it collapses. This is the chat-app layout, not the search layout
  (`patterns-layout-grid.md`): the results-header pill at the top of the
  column does not exist here, everything funnels through the bottom pin.
- Material 3 expressive: fully-rounded pill surfaces, tonal elevation via
  soft shadows rather than borders, large corner radii on menus and
  dialogs, Google Sans for display text (a per-product divergence flagged
  in `patterns-typography.md`), body text near 16px/1.6 in near-black warm
  ink; headings can reach 700, above the 600 ceiling most other products use.
- Gemini gradient/branding: the blue-to-lavender-to-pink gradient appears as
  the four-color sparkle star, in "thinking" state tints, and faintly in the
  landing background. Brand color is atmosphere (large soft washes), not
  functional color; links and citations stay near-monochrome.
- Dark mode: inverts to near-black canvas with the same gradient desaturated
  into deep blue/purple washes; the pill becomes a dark-elevated surface.
- Composer anatomy (final form, pinned bottom): `+` attach, text field,
  model picker text-button, mic; during streaming a stop control occupies
  the submit slot; the disclaimer line sits just below the pill. Multiple
  capabilities (Canvas, Deep Research, video) enter through tools attached
  to this one composer rather than separate pages.

## Verdict for cauce

1. Do not borrow the answer-only frame. Gemini can make the landing the app
   because it has no classic mode; cauce's shared top-of-column pill, mode
   toggle, and `view Search`/`ask AI instead` hatches exist precisely because
   cauce is not this. Keep `search.md`'s layout contract.
2. Borrow the composer morph as a transition, not a layout: animating the
   same pill from hero position to its results position in one continuous
   motion is a legitimate upgrade to cauce's landing->search handoff, as long
   as cauce's pill lands at the top of the column, not bottom-pinned
   (patterns-layout-grid.md records bottom-pin as an intentional divergence).
3. The collapsible streamed "Thoughts" region is the strongest thinking-state
   pattern observed: it streams real progress, stays out of the way when
   collapsed, and never blocks the answer. Prefer it over cauce's static
   skeleton if the cauce backend can expose reasoning/phase events; otherwise
   keep the skeleton plus status line from `patterns-states.md`.
4. Grounding chips ("Searched for ...") above the answer plus a source strip
   below are a good, cheap richness step for cauce's AI mode: show the queries
   the backend actually ran (cauce already has them), keep inline [n] citation
   markers at claim points per the existing streaming contract, and keep
   `view Search` as the stronger escape hatch Gemini cannot offer.
5. Keep Gemini's gradient-atmosphere branding and Google Sans at arm's
   length: they are single-product identity choices, and cauce's daisyUI
   token set (`tokens.md`) plus system fonts are the cross-product converged
   position.

## Sources

- `shots/gemini-landing-idle.png` (hero greeting, pill anatomy, gradient canvas)
- `shots/gemini-answer-complete-pinned-composer.png` (bottom-pinned composer,
  disclaimer line, action row, inline grounding chip "Selekt Audio",
  completed-answer typography)
- https://blog.google/products-and-platforms/products/gemini/tips-how-to-use-deep-research/ (Deep Research plan/edit-plan flow, "Show thinking" and "Sites browsed")
- https://9to5google.com/2025/02/10/gemini-2-0-experimental-models-app/ ("Thoughts" section, streamed reasoning behavior)
- https://ai.google.dev/gemini-api/docs/generate-content/google-search (groundingMetadata, groundingChunks/Supports span citations, searchEntryPoint chips)
- https://ai.google.dev/gemini-api/docs/interactions/google-search (inline url_citation annotations, per-query grounding)
- https://emulent.com/resources/how-gemini-chooses-sources/ (query fan-out, per-claim citation binding)
