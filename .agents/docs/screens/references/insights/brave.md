# Brave Search: deep UI/UX analysis

Per-product insight doc on search.brave.com (classic SERP, AI Answers, Ask Brave,
Goggles). Follows the README conventions. Named "Answer with AI" through 2025;
Brave renamed it "AI Answers" with the September 2025 "Ask Brave" launch. This
doc uses "AI Answers" for the current inline block and "Ask Brave" for the
chat surface.

## Applies to

`search.md` Mockup A/B (classic SERP anatomy, AI block placement, escape
hatch), `landing.md` (rich input pill), `userflow-checkpoints.md`
checkpoints 3, 4, 9, 10, 11, 13, 14. Cross-references
`../patterns-hybrid-serp.md` (inline block shape) and
`../patterns-result-list.md` (card anatomy).

## 1. Pages and surfaces

- **Landing** (`shots/brave-landing-idle.png`): an extremely compressed hero.
  Lion mark plus lowercase "brave" wordmark centered around the upper third, one
  full-width pill below it at roughly 65% viewport width. The pill is unusually
  rich for a landing: a "+" attach button on the left (file / image attach for
  AI mode), the input with placeholder "Ask anything, find anything...", a
  microphone icon, and an "Ask" button with a sparkle glyph pinned to the right
  edge as a soft pill. Brave collapsed the classic search-vs-AI choice into one
  input where "Ask" is the explicit action button, not a mode segment. Compared
  with oxe's landing spec, Brave spends more chrome on the pill itself and has
  essentially no tagline or starter prompts visible at idle.
- **Classic SERP**: sticky header with the wordmark small at left, the pill at
  top of the content column, and a horizontal tab row under it (All, Images,
  Videos, News, Goggles, plus "Ask" as a peer tab since Sep 2025). Result column
  is left-anchored, roughly 600 to 650px at desktop; the right side of the
  viewport stays empty or hosts an infobox/entity panel. Results follow the
  convergent anatomy: favicon plus domain line, blue title link, two-line
  snippet, whitespace-only separation. Brave adds a per-result action cluster
  (Rerank thumbs up/down) on hover or in a results-page corner panel.
- **AI Answers block**: renders at the top of the classic SERP for qualifying
  (question-like) queries, above organic results, inside a visually distinct
  card with a subtle border and slightly tinted background. Answer text with
  inline citation markers, source chips, and sometimes generated entity cards or
  images interleaved (Brave calls these "enrichments"). An on-demand trigger
  (sparkle "Answer with AI" / "Ask" icon beside the pill) forces generation on
  queries that did not qualify.
- **Ask Brave** (`search.brave.com/ask`): a distinct chat surface reached via
  the Ask button, the Ask tab, or a "???" suffix typed into the query. It is the
  Google AI Mode analog rather than the inline block: full-page answer column,
  chat follow-ups, and "transparent research steps" shown while Deep Research
  mode works (visible multi-query fan-out, dozens of queries, thousands of
  pages).
- **Discussions / News / Forums verticals**: News and the forum-discussion
  modules exist both as tabs and as augment modules inside AI answers (Brave's
  system instructions retrieve `forum_discussions` and `news_articles` per
  section). Discussions surface community threads (Reddit etc.) as a grouped
  list, not as a differently-anatomied result; the anatomy stays the classic
  favicon+title+snippet.
- **Goggles**: a peer tab on the SERP, unique to Brave. Goggles are community
  re-ranking instruction files (boost/downrank/discard by URL pattern) applied
  on top of the index; a Discovery page lists them, following one stores it in
  browser localStorage and applies it privately per query. "Rerank" (Jan 2025)
  is the consumer simplification: a panel over results with thumbs up/down per
  domain, stored device-side only. This is the strongest "ranking transparency"
  differentiator in any harvested product and the conceptual cousin of oxe's
  cache-transparency requirement.
- **Settings**: reached via a gear at top right of the SERP. Flat list, includes
  the "Answer with AI" toggle (also mirrored in Brave browser settings). Notably
  the setting is buried under "All settings" and, per a long-running
  brave-browser issue, turning it off does NOT hide the Ask/AI icons in the pill;
  users resort to element blockers. Off means off for the block, but the
  affordances stay.

## 2. State transitions

- **AI Answers generation**: inline block appears in the results slot with a
  compact thinking state, then streams. Brave reports single-search answers
  stream in under ~4.5s on average, so the thinking phase is short and there is
  no Perplexity-style multi-phase status pill; the block shows a small loading
  indicator then text arrives progressively. Ask Brave's Deep Research mode is
  the opposite: it surfaces named research steps (search queries issued) while
  iterating, which is the best-in-harvest example of "progressing status line"
  instead of a silent skeleton.
- **Opt-out**: default on for qualifying queries; disabled via the settings
  gear ("Answer with AI" toggle), not a per-query dismiss. After opting out the
  SERP is purely classic; the residual pill icons are a documented annoyance
  (GitHub issue 48347). Brave's framing is "when you need them": AI is
  automatic for question-like queries, on-demand via the sparkle icon otherwise.
- **Fallback when no answer**: if a query does not qualify for generation, the
  block simply never renders; the organic list is the whole page and the sparkle
  icon remains as the manual trigger. There is no explicit "no AI answer
  available" state; absence is the fallback. Contrast oxe, which specifies an
  explicit empty-sources failure state; Brave's silent-absence approach is
  viable because organic results always share the page (inline-block shape),
  whereas oxe's distinct AI surface needs the louder failure copy already in
  `patterns-states.md`.
- **Follow-ups**: AI Answers block itself is not conversational ("by design a
  bit less interactive", per the engineering interview in the 2024 blog post);
  follow-ups route to Ask Brave. This is a clean two-tier split oxe could note:
  quick inline answer, explicit escalation to chat.

## 3. Animations and motion

- No public motion token set; inferred behavior from live use and API docs.
  Single-search streaming is fast (<4.5s target) so text appears in quick
  progressive chunks rather than token-by-token theater; there is no persistent
  decorative cursor on the inline block. Ask Brave streams chat-style with a
  working indicator during research steps.
- Thinking treatment is a small inline spinner/text in the answer slot, not a
  full-area skeleton; the organic results below load immediately and
  independently, so the AI block's latency never blocks the page's first paint.
  That decoupling (AI block lazy-fills its reserved slot while classic results
  render) is worth copying: oxe's AI mode should not hold Search-mode render
  hostage to model latency.
- Entity/image enrichments pop in as the stream resolves them (the backend
  detects entity types in the token stream and fetches cards on the fly per the
  2024 engineering interview), meaning the completed block can grow after text
  finished; Brave reserves loose space rather than eliminating all shift.
- No evidence of reduced-motion-specific handling in any public doc; do not
  treat Brave as a reference for the reduced-motion contract, oxe's own
  `patterns-motion.md` is stricter.

## 4. AI retrieval and source richness

- **Citation mechanics**: inline numbered markers tied to source chips under or
  beside the answer text. The API contract (Brave Answers endpoint) streams
  citations as structured `<citation>` events carrying start/end character
  offsets, number, url, favicon, and snippet, i.e. citations are positioned
  data, not appended links. This matches the "typed into the text at the point
  of the claim" convention in `patterns-answer-streaming.md` and is strong
  evidence that oxe's `[n]`-at-claim spec reflects how a major engine
  implements it internally.
- **"Progressive citations"** is an explicit Brave API feature name: citation
  events arrive interleaved with text deltas during the stream, so source
  chips populate as claims land. Supports the oxe reveal order (sources appear
  during, not only after, streaming).
- **Source selection is query-level**: Answer with AI analyzes a whole page and
  picks paragraph/sentence/table-row level context (their words), so the cited
  set is finer grained than the ten blue links below it. In Ask Brave the
  links beside a paragraph come from a second retrieval (per-section augment
  blocks like `web_results`, `videos`, `news_articles`) using model-authored
  queries, and Brave's own system instructions forbid ending with a "Further
  Exploration" link dump. Consequence: source modules are per-section, richer
  than Google AI Mode's single side rail in variety (videos, news, shopping,
  local, forums) though less unified in presentation.
- **Multiple perspectives as policy**: grounding docs state the answer should
  surface nuance and contradictions rather than collapse to one fact (their
  SimpleQA "wrong" answers are deliberately richer). Copy implication: hedging
  is banned but disagreement between sources is shown, not averaged.
- **vs Google AI Mode**: Google uses a dedicated right-side sources rail with
  rich cards; Brave keeps sources as compact chips plus per-section augment
  modules inline, closer to the page's classic visual language. oxe's
  horizontal source-card row sits between the two and is fine.

## 5. Layout and design system

- **Columns**: classic results ~600 to 650px left-anchored; Ask Brave answer
  column centered prose around ~680px; enrichments (image strips, video rows)
  can widen to the viewport with the text column capped. Landing pill max-width
  is wider than oxe's (~750 to 800px) because of its attach button cluster.
- **Palette**: Brave's signature is the orange lion gradient (roughly #FF3F1E
  to #B8330C range) on near-white; accent usage on the web app is restrained,
  mostly the wordmark, the Ask sparkle, and focus/active states. Link titles
  stay blue in classic results, not orange; Brave does not tint the whole SERP.
- **Dark mode**: full parity dark theme on search.brave.com (system-following
  plus manual toggle in settings), dark surfaces are true dark gray rather than
  pure black, answer blocks stay slightly elevated cards in both themes.
- **Result card anatomy**: convergent classic anatomy, no boxed cards, one
  Brave-specific addition is the Rerank affordance (thumbs up/down arrows) at
  the result or panel level, and an occasional "Discussions" or video module
  inserted as a grouped unit between organic results.
- **Extensionless look**: search.brave.com is a standalone web app, no browser
  chrome assumptions; header is sticky, tabs sit under the pill, and there is
  no footer on results pages, matching `patterns-layout-grid.md`. Typography is
  a system-adjacent sans (Brave uses its own licensed faces in product, but the
  web search app reads as a humanist sans ~15 to 16px body, semibold titles);
  no exotic display type on the SERP.

## Verdict for oxe

1. Keep the two-tier AI split in mind: Brave proves an inline quick answer plus
   an explicit escalation to chat scales better than forcing one surface to do
   both. oxe's `ask AI instead` / `view Search` hatch plays the same role and
   should stay cheap and always visible.
2. Copy Brave's decoupled loading: classic results render immediately while the
   AI block fills its own slot; never block one mode's first paint on the
   other's model latency.
3. Brave's API streams citations as offset-positioned events interleaved with
   text ("progressive citations"), which validates oxe's spec of inline `[n]`
   markers appearing during the stream rather than a post-hoc source list.
4. Silent-absence fallback works only for inline-block products; since oxe's AI
   mode is a distinct surface, keep the explicit empty-sources and mid-stream
   failure states from `patterns-states.md` instead of imitating Brave's quiet
   no-op.
5. Goggles/Rerank is the best existing precedent for user-visible ranking
   control (boost/downrank/discard, stored locally, transparent); oxe's cache
   transparency should adopt the same "panel over results, device-side, undoable"
   interaction grammar.

## Sources

- Shot: `shots/brave-landing-idle.png` (landing hero, rich pill with Ask button)
- https://brave.com/blog/answer-with-ai/ (2024 launch: inline block placement,
  on-demand trigger, entity enrichments, grounding interview)
- https://brave.com/blog/ask-brave/ (Sep 2025: Ask Brave, renaming to AI
  Answers, research steps, 15M answers/day, settings opt-out note)
- https://brave.com/blog/ai-grounding/ (Aug 2025: AI Grounding, SimpleQA
  methodology, multi-perspective policy)
- https://api-dashboard.search.brave.com/documentation/services/answers
  (streaming citation event format with offsets, favicons, progressive
  citations, research mode)
- https://search.brave.com/help/goggles and
  https://api-dashboard.search.brave.com/documentation/resources/goggles
  (Goggles DSL, localStorage application, limits)
- https://brave.com/blog/search-rerank/ (Rerank panel, Jan 2025)
- https://github.com/brave/brave-browser/issues/48347 (opt-out leaves AI icons;
  community pressure on the toggle)
- https://dejan.ai/blog/brave-ai-search/ (2026: leaked system instructions,
  augment blocks as per-section citation mechanism, no link-dump rule,
  sub-4.5s streaming target)
