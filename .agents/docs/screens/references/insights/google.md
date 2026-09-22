# Product insight: Google Search and Google AI Mode

A per-product deep dive (see README's patterns-not-targets rule: this is
anecdote-rich by design, so nothing here graduates to a "general" rule
without corroboration in the `patterns-*.md` docs). Focus: Google's classic
SERP, AI Overviews as an inline block, and AI Mode as a distinct surface,
since cauce ships both shapes.

## Applies to

`landing.md` (hero pill, suggestions), `search.md` Mockups A/B/B2/C
(Search mode, AI streaming, AI complete, error states), checkpoints 1 to 6
and 9 to 14 in `userflow-checkpoints.md`. The right-rail Sources analysis
informs `search.md`'s source-card row decision (rail vs horizontal scroll).

## 1. Pages and surfaces

**Landing** (`google-landing-full-idle-loggedout.png`,
`google-landing-full-idle-loggedin.png`): the most austere landing in the
harvested set. Logo at roughly 30% viewport height, one pill input centered
below it, two dead-flat grey buttons (`Google Search`, `I'm Feeling Lucky`)
with 8px radius (not pills), one line of localized footer text. The pill is
~584px wide, ~44px tall, radius 9999px, with a hairline border and a very
soft resting shadow. Logged-in, the pill carries a greeting wordmark
("Good afternoon") instead of the logo. Notably the AI entry point already
lives on the landing pill: an `AI Mode` chip with a sparkle-magnifier icon
sits inside the pill's right end (`google-aimode-composer-idle-closeup-2.png`,
`google-aimode-composer-idle-closeup.png`), alongside mic and Lens icons and
a `+` attach button on the left. That is the in-composer toggle model from
`patterns-search-input.md`, shipped by the biggest search engine.

**Classic SERP**: header with pill input pinned top-left of the content
column, tab row (`All Images Videos News ...`) directly under it with an
active-tab underline, then a left-anchored ~600 to 650px result column; the
right side hosts the Knowledge Panel at wide viewports. Results are
unboxed: favicon plus breadcrumb domain line, 20px title link in
`#1a0dab`, up to two lines of snippet, occasionally a sitelinks sub-list.
Separation is whitespace only, no card chrome.

**AI Mode surface** (`google-aimode-answer-complete-twocolumn.png`): a
fully distinct view, not an inline block. The result is a chat transcript:
the user query echoes as a right-aligned grey pill at the top of the
answer column, the answer streams below in a wide left column (~620px
measured against the shot's 1568px width), and the follow-up composer is a
large rounded rectangle pinned at the bottom of the answer column with
`Ask anything` placeholder, `+` attach, and mic. History is reachable from
a left sidebar.

**AI Overviews inline block** (not captured in shots; described from web
research): on qualifying classic queries, a visually distinct panel is
injected above the organic results with a Gemini sparkle label, the answer
text, inline citation chips, and horizontal source-card rows; organic
results continue below. It is the "inline block above organic results"
shape from `patterns-hybrid-serp.md`, same family as DDG Search Assist and
Brave Answer with AI.

**Images / Videos tabs** (brief): Images replaces the list with a dense
justified masonry grid, chromeful and visually loud; Videos shows
thumbnail cards with duration badges and source chips. Google treats these
as genuinely different layouts, not re-skins of the list, which supports
`patterns-layout-grid.md`'s core claim that a result list is not one
universal container.

**Suggestions dropdown** (`google-aimode-suggestions-dropdown-typed.png`,
`google-aimode-idle-example-prompts.png`): the dropdown is fused to the
pill (shared rounded container, divider line under the input row, not a
floating detached panel). Two behaviours worth noting: (a) while the input
is empty on the AI surface, Google shows example prompts with the
sparkle-magnifier icon, the "empty state teaches the input format" pattern
from `patterns-states.md`; (b) once typed, predictions render with the
typed prefix in regular weight and the completion in bold
(`sample search` + `**warrant**`), one row per prediction, each with the
same leading icon. A muted `Report inappropriate predictions` /
`Learn more` pair anchors the dropdown's bottom right.

## 2. State transitions

Classic path: idle (placeholder `Search Google or type a URL` era is gone;
now icon-led) -> typed (predictions appear, submit is implicit via Enter
since the buttons only exist on the landing) -> submitted (full page
navigation; Google still does a hard navigational model with URL change to
`/search?q=`) -> loading (tab row and results shell render before data;
progress via the browser, no custom skeleton) -> rendered.

AI Mode path: idle -> typed (suggestions, `AI Mode →` submit button turns
filled blue) -> submitted: the query echo pill appears immediately
(optimistic, matches `patterns-answer-streaming.md` order of reveal step
1) -> a thinking phase with an animated sparkle/thinking indicator before
the first token (Google's "query fan-out" work happens here, see section
4) -> streaming with markdown headings and bold forming progressively ->
complete: citation chips stay clickable, a source count / link affordance
remains -> follow-up: the composer at the bottom accepts the next turn and
the transcript grows downward; the composer never scrolls away, matching
`patterns-search-input.md`'s "input always visible" contract.

Failure paths Google does show: when confidence is low, AI Mode falls back
to rendering a set of web links instead of an answer (documented in
Google's own help pages), which is a graceful degradation cauce should mimic
with `view Search` rather than an error.

## 3. Motion

- Suggestion dropdown: appears effectively instantly (0 to 80ms) attached
  to the pill; no slide or scale that I can detect in the captures, rows
  highlight with a flat grey fill on hover/keyboard focus with a fast
  (~120ms) background transition.
- AI streaming: token cadence is tied to real generation, no typewriter
  pacing. Bold and heading formatting resolve as structure closes; no
  half-rendered markdown garbage is visible, implying they hold back
  still-forming fragments (the same tail-stripping concern behind cauce's
  fixed `jsonleak` bug).
- Thinking indicator: an animated sparkle/glimmer treatment on the
  Gemini icon while the fan-out runs, plus a pulsing "thinking" shimmer on
  the answer area before the first token. This is Google's equivalent of
  the phase status line in `patterns-states.md`, but expressed as a
  branded shimmer rather than text; the shimmer runs longer than 2s on
  hard queries without escalating copy (a divergence from the escalation
  rule, worth noting not copying).
- Tab underline: slides between tabs (~220ms standard easing) on the
  classic SERP tab row.
- Reduced motion: Google honors `prefers-reduced-motion` broadly (the
  homepage doodles and shimmer animations collapse to static); no
  AI-Mode-specific reduced-motion documentation is public. The blink of
  any streaming caret is standard ~530ms opacity step.

## 4. AI mode retrieval and source richness

Mechanics first, because they explain the UI: Google's "query fan-out"
rewrites one user question into roughly nine or more synthetic subqueries
(published analyses count up to hundreds of underlying searches, capped
around 20 retrieval iterations), retrieves passages densely, re-ranks
pairwise, and narrows 200 to 500 candidate documents down to about 5 to 15
cited sources, of which only 3 to 4 are usually visible at first
(corymaki.com, becited.io, blog.google). The UI consequence: **the visible
citation surface is a deliberate compression of a much larger retrieval**,
which is why the Sources rail exists.

What the UI shows (`google-aimode-answer-complete-twocolumn.png`):

- **Inline citation chips**: at the point of a claim, a small rounded chip
  appears with the site favicon plus the site name (e.g. the WhoSampled
  chip after the first bullet, `YouTube · Dan Carasco +1` after the
  second). This is richer than a bare `[n]`: favicon plus domain plus an
  overflow counter for additional sources behind the same claim. Blue
  link text inside the answer is reserved for the primary referenced
  entity (product names), while chips mark supporting sources. This is a
  meaningful upgrade over cauce's planned `[n]` markers: favicon chips
  carry trust at a glance without reading the domain.
- **Right-hand Sources rail**: a distinct card column (~420px) next to the
  answer with rich source cards, one per cited site: favicon plus site
  name and a kebab menu on the header row, a real headline title (2
  lines), a snippet line prefixed by an actual date (`16 Sept 2026`),
  and a thumbnail on the right (screenshots, video frames with duration
  overlay for YouTube). Cards are separated by hairline dividers inside a
  single rounded container. This is dramatically richer than the
  horizontal favicon strip that most AI search products (and cauce's
  current spec) use, and it works because Google has thumbnails and
  real titles for every citation.
- **Show all**: a full-width pill button at the rail's bottom expands the
  truncated card list, the collapse/expand pattern in
  `patterns-motion.md` item 4.
- **Follow-ups**: on the complete answer, suggested follow-up questions
  render as tappable chips/list items after the answer (Google's help
  pages describe the flow; the captured shot shows the composer ready for
  a free-form follow-up instead). Follow-ups are conversational turns, not
  new searches: session context persists (stateful-chat patent).
- **Relationship to classic results**: sources in the rail frequently
  include Reddit, YouTube, and niche forums that would also rank
  organically, but only about 38% of cited pages sit in the organic top
  10, so the rail is not just "the first four results restyled"; it is a
  separately ranked set. For cauce this justifies deriving AI sources from
  the actual search backend response rather than assuming they mirror the
  classic list.

## 5. Layout and design system

- **Columns**: landing pill ~584px centered; classic SERP result column
  ~600 to 652px left-anchored under the header pill with the right side
  reserved for the Knowledge Panel; AI Mode splits the space into an
  answer column (~620px) plus a Sources rail (~420px) with a large gutter,
  i.e. AI Mode is the one Google surface that is genuinely two-column.
  Confirms `patterns-layout-grid.md`: list and answer are different
  containers, and Google even gives them different page scaffolds.
- **Type scale**: Roboto/Google Sans throughout. Landing pill text 16px;
  classic title links ~20px/400 (weight stays 400, color alone signals
  link); snippet 14px `#4d5156`; AI Mode answer body 16px/400 with
  bold 700 in-answer emphasis and 22 to 24px/600 section headings
  (`To Identify a Sample...`); query echo pill 14 to 16px in a muted grey
  container. Note the divergence from `patterns-typography.md`'s "query
  echo is LARGER than the answer" convergence: Google shrinks the echo
  into a pill, because the echo is conversational there, not a heading.
- **Color roles**: link blue `#1a0dab` (visited `#681da8`) on classic;
  the only saturated accent elsewhere is the AI blue `#1a73e8`-family
  used for the filled `AI Mode` button and focused states; snippet/domain
  grey `#4d5156` and `#202124` body text; dropdown/hero surfaces pure
  white on white canvas with `#dadce0` hairlines. Dark mode (not captured
  for classic SERP, but documented): body `#e8eaed`, link blue brightens
  to `#8ab4f8`, surfaces `#202124`/`#303134`, matching the dark-link
  value already assumed in rubric row 2.2.
- **Spacing**: result vertical rhythm ~20 to 26px between results, 8px
  radius on every button/chip except the 9999px pill; card surfaces are
  rare (AI Mode Sources rail container and the query echo pill are the
  main ones) and always use very soft shadows, near-flat.
- **Result card anatomy (classic)**: favicon 16px + domain breadcrumb ->
  title link (only saturated text) -> 2-line clamped snippet -> optional
  sitelinks/metadata. Exactly the anatomy in `patterns-result-list.md`.
- **AI answer block anatomy**: query echo pill -> streamed prose (bold
  entities, section headings, bullets) with inline favicon citation chips
  at claim points -> (in the captured two-column layout) Sources rail to
  the right with `Show all` -> follow-up composer pinned at the bottom.
  The disclaimer/dattery text Google appends under AI Overviews ("AI is
  experimental") sits directly under the answer, never in a page footer,
  matching `patterns-layout-grid.md`'s footer rule.

## Verdict for cauce

1. **Copy the in-composer AI chip, keep it visible everywhere**: Google
   puts the `AI Mode` chip inside the pill on the landing, the SERP
   header, and the AI surface alike; cauce's segmented control should behave
   as one persistent control per `patterns-search-input.md`, not a
   page-level tab.
2. **Steal the favicon citation chip with overflow counter** (`favicon +
   domain +1`) instead of bare `[n]` text markers if source favicons are
   available from DuckDuckGo results; fall back to `[n]` when they are
   not. This is the single most transferable AI Mode idea for cauce.
3. **A right-hand Sources rail needs thumbnails and real titles to pay
   off; cauce has neither reliably, so keep the horizontal compact-card row
   per the current spec**, and consider `Show all` expansion on it (that
   affordance works at any richness level).
4. **Skip Google's branded shimmer-only thinking phase**: a sparkle
   shimmer with no text reads as frozen past 2s; keep cauce's textual
   progressing status line from `patterns-states.md`.
5. **Copy the low-confidence fallback**: when the AI answer is not
   available, Google silently renders web links; cauce should render classic
   results with the muted `AI mode is not configured` notice per
   `patterns-states.md` 6.8 rather than an error surface.

## Sources

Shots (`.agents/docs/screens/references/shots/`):

- `google-landing-full-idle-loggedout.png`, `google-landing-full-idle-loggedin.png`
- `google-aimode-composer-idle-closeup.png`, `google-aimode-composer-idle-closeup-2.png`
- `google-aimode-composer-dark-idle.png`
- `google-aimode-idle-example-prompts.png`
- `google-aimode-suggestions-dropdown-typed.png`
- `google-aimode-answer-complete-twocolumn.png`

Web:

- https://blog.google/products-and-platforms/products/search/ai-mode-search/ (AI Mode intro, query fan-out, confidence fallback)
- https://blog.google/products-and-platforms/products/search/google-search-ai-mode-update/ (I/O 2025: AI Mode tab rollout, Deep Search, agentic mode)
- https://support.google.com/websearch/answer/16011537 (AI Mode help: follow-ups, fallback to web links, Gemini 3 generative UI)
- https://www.searchenginejournal.com/query-fan-out-technique-in-ai-mode-new-details-from-google/ (Stein interview on fan-out mechanics)
- https://corymaki.com/how-google-ai-overviews-choose-citations-query-fan-out/ (candidate narrowing 200 to 500 down to 5 to 15 cited, 3 to 4 visible citations, 38% top-10 stat)
- https://becited.io/ai-search-guide/how-ai-mode-works (patent-based breakdown: fan-out, pairwise ranking, stateful chat)
