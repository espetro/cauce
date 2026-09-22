# Deep UI/UX analysis: DuckDuckGo (classic SERP, Search Assist, Duck.ai)

Product analysis, not a target spec. Per `references/README.md`, single-product
observations stay here as anecdote; only convergence across 2+ products becomes a
general rule in `patterns-*.md`. This analysis matters doubly for cauce because cauce's
search backend IS DuckDuckGo: anything DDG does at the pipeline level (snippets,
favicons, result anatomy, rate-limit surfaces) shapes what cauce's UI can honestly
display.

## Applies to

- `landing.md` (hero input, mode toggle, Duck.ai-style idle states)
- `search.md` (classic SERP anatomy, AI answer surface, citations, states)
- `history.md` (recent chats / local-only history model)
- Rubric sections: 1 Input, 2 Result list, 3 Answer surface, 5 Layout, 6 States, 7 Motion

## 1. Surfaces

### 1.1 Landing (`duckai-landing-idle.png`, `duckduckgo-classic-input-idle.png`)

Duck.ai landing is radically sparse: logo mark, one line of copy ("All chats are
private" with "private" underlined as the only link), and the composer pill. No
example prompts, no onboarding, no settings maze (TechRadar confirms "no dashboard,
no settings maze, no onboarding flow"). The entire privacy pitch is one sentence
with one underlined word. Compare cauce's landing which carries a wordmark, tagline,
and mode segmented control; DDG lands the same trust message with far less.

The classic DDG homepage input (`duckduckgo-classic-input-idle.png`) is a plain
rounded rectangle, 1px grey border, white fill, magnifier icon right, placeholder
"Search DuckDuckGo". Notably it is a rectangle (moderate radius, roughly 10-12px),
not the full pill Duck.ai uses. DDG uses two different input shapes for its two
surfaces.

### 1.2 Classic SERP with Search Assist (`duckduckgo-serp-search-assist-dark.png`)

From top: full-width sticky header containing the query pill (this is the
continuous input, always present); a tab row (All / Images / Videos / News / More)
with "Search Assist" and "Duck.ai" as labelled entries on the right plus a gear
icon; a filter row (Private badge, region toggle, Safe search, Any time); then the
Search Assist card; then organic results in a single left-anchored column.

Search Assist appears above the first organic result as a rounded card (~24px
radius) on a slightly elevated surface. Header row inside the card: sparkles icon +
"Search Assist" label left; copy, share, info, settings icons right. Body is 3-5
lines of prose with inline domain chips at the end (see section 4). Footer row
inside the card: a centered "More" chevron button flanked by hairline rules, and a
circular Duck.ai button to its right, the upgrade path from snippet to full chat.

Below the card, outside it: "Auto-generated based on listed sources. May contain
inaccuracies." left, "Was this helpful?" with thumbs right. The disclaimer is
outside the card boundary, visually quieter than the card itself.

### 1.3 Duck.ai chat thread (`duckai-answer-complete-privacy-banner.png`)

Structure top to bottom: the privacy banner (a thin full-width bar, lock icon +
"Anonymized by DuckDuckGo. Zero provider visibility. No AI training. Learn more");
the user message as a full-width grey rounded block with copy/edit icons below
right; the assistant message starting with a model identity chip ("Gemma 4 31B"
with provider logo); answer body; per-message action row (copy, download, more on
the left, thumbs up/down on the right); large empty space; pinned composer at the
bottom with "Reply..." placeholder, image attach, Tools, Fast toggle, send button.

### 1.4 Settings

DDG has no visible settings surface inside Duck.ai beyond the gear icons on Search
Assist (which toggle the feature / open model settings). Settings live on the main
duckduckgo.com settings pages, separate from AI. The composer-level controls (model
dropdown, Fast/Tools) are the only in-context configuration. This is a deliberate
friction-reduction choice consistent with "no settings maze".

## 2. State transitions

### 2.1 Search Assist appearance logic

Search Assist is opt-out-able and query-gated: it does not render for every query.
DDG shows it when the query looks informational/conversational; navigational and
short head queries get plain results. When it does not auto-appear, the "Search
Assist" entry in the tab row (highlighted, with a pill background in the dark shot)
lets the user summon it. The "More" chevron expands the collapsed 3-5 line preview
to a longer answer inline without leaving the SERP; the circular Duck.ai button
hands the whole query to the full chat. So the ladder is: collapsed snippet ->
expanded snippet -> full chat, each step user-initiated.

### 2.2 Duck.ai streaming and message lifecycle

Streaming follows the standard token cadence (see `patterns-answer-streaming.md`);
DDG adds nothing exotic. Notable states:

- The composer's send button is the submit; during streaming the input accepts the
  next message but the model chip stays visible above the streaming answer so the
  user always knows which model is producing text.
- After completion, per-message actions reveal (copy, download, more, thumbs).
  DDG surfaces download of the answer as a first-class action, which most chat UIs
  bury in an overflow menu.
- Edit on the user message (pencil icon) forks/resubmits the turn.

### 2.3 Privacy banner

The "Anonymized by DuckDuckGo" bar sits at the very top of the thread, above all
content, persistent for the session. It is reassurance placed exactly where doubt
peaks (right after the user typed something sensitive). Wording is concrete and
falsifiable: "Zero provider visibility. No AI training." with a "Learn more" link.
The landing compresses the same promise to "All chats are private".

### 2.4 Recent chats and resume

Per the ClickUp and TechRadar reviews (2026): Recent Chats live in a left sidebar,
stored locally on the device, never on DDG servers; Sync & Backup is opt-in. A
reopened thread continues with full in-thread context but there is no cross-chat
memory. Model can be switched mid-conversation; the thread just continues. For
cauce's `history.md` this is the closest precedent for a local-only, privacy-framed
history surface: history framed as a feature of privacy ("stored on your device"),
not as surveillance.

### 2.5 Model selector and Fast/Deep

The model is shown as a labelled dropdown inside the composer ("Gemma" with
chevron, `duckai-landing-idle.png`), i.e. the current selection is always visible,
not hidden behind an icon. The dropdown groups models by lab and annotates cost
tier and reasoning support (help pages publish a per-model table: Limit Cost,
Reasoning, Extended Reasoning). The "Fast" lightning toggle in the composer is a
speed/depth switch (fast answers vs deeper reasoning); "Tools" exposes per-turn
capabilities (image input, web search). Per-turn controls live in the composer,
never in a settings page.

### 2.6 Rate limiting

DDG enforces usage limits per model cost tier, resetting on intervals (help pages).
UX treatment: the limit surfaces inline in the chat as a message-level notice with
reset timing, not as a modal, and paid tiers raise limits. This matches
`patterns-states.md`'s calm rate-limit treatment; cauce's "Rate limit reached,
retrying in Ns" line is consistent with DDG's approach.

## 3. Motion

DDG is famously calm and it is worth documenting the restraint as a choice:

- What animates: Search Assist expand/collapse (the "More" chevron), streaming
  token reveal, standard hover color shifts. Nothing else.
- What deliberately does not: the SERP results do not fade or stagger in on page
  load; the Search Assist card appears without a reveal animation once rendered;
  tab switches are instant; landing-to-thread is a hard transition. There are no
  skeletons on the classic SERP at all.
- The only looping motion anywhere in the product is the streaming caret. DDG
  treats any motion beyond that as spending trust, and its whole brand is trust.

For cauce this validates the rubric's anti-pattern list (7.6): motion budget goes to
streaming feedback, not decoration. DDG is the lower bound of the convergence range
in `patterns-motion.md`; Google AI Mode is the upper bound.

## 4. AI retrieval and source richness

### 4.1 Search Assist citations

Weak-to-moderate richness. Citations are domain chips (favicon + bare domain,
e.g. "splice.com", "sononym.net") attached to the end of the answer paragraph,
grouped in one pill row. There are no inline numbered markers `[1]` typed into the
prose at the point of each claim; there is no per-claim mapping at all. The
disclaimer "based on listed sources" is accurate: the sources are a list, loosely
related to the whole answer. This is thinner than Google AI Mode / AI Overviews,
which place numbered inline markers per claim and a source carousel above the
answer (see `patterns-hybrid-serp.md` comparison). Search Assist trades citation
precision for visual calm.

### 4.2 Duck.ai citations

Duck.ai historically had no citations (pure chat). With recent updates it performs
web search on demand and returns sources, but the treatment is closer to footnotes
the model emits than to a UI-level source system: there is no persistent source
panel, no numbered-marker-to-card highlighting like cauce's rubric 3.5 requires. For
cauce this is a gap DDG leaves open: cauce's inline `[n]` markers linked to source
cards (rubric 3.5, 3.6) are richer than anything DDG ships, and since cauce's backend
returns the actual DDG result set, cauce can wire citations to real fetched documents
in a way Duck.ai's proxy architecture does not surface.

### 4.3 Backend-relevant notes for a DDG-backed pipeline (cauce-specific)

- Snippets: DDG result snippets are plain text with bolded query terms (visible in
  the dark SERP shot: "Sample" bolded inside the snippet). cauce receives this and
  should preserve the bold-term highlighting in its result list.
- Favicons: DDG SERP shows favicons inline with the https://domain line at ~16px.
  cauce must handle the same favicon-availability problems (DDG serves them via its
  icon service; failures need a graceful letter fallback per rubric 2.1).
- Sitelinks: DDG augments the first result with a multi-column sitelinks block
  (About / Blog / Funk Soul / Ambient... in the shot). cauce's DDG backend does not
  return sitelinks; cauce's flat result list is honest to its data, do not fake it.
- Rate limiting: DDG intermittently throttles automated/anonymous queries (CAPTCHA
  or 403 on the HTML endpoints). cauce's cache exists partly to absorb this; the UX
  implication is that cauce's `cached · <age>` badge and "served from cache" honesty
  (rubric 2.5, 3.8) should also become the fallback surface when DDG throttles: a
  stale-cache-with-age-notice state beats a raw error. This state is not in
  `patterns-states.md` yet and is worth adding as a cauce-specific finding.
- No result-count or timing metadata comes back reliably from DDG; cauce's meta line
  should not fabricate "about N results".

## 5. Layout and design system

- Content column: the classic SERP column is roughly 652px, left-anchored within
  the page with the header input full-width above it. Duck.ai's thread column is
  similar (~650-680px), centered as a chat column. This is the direct source of
  cauce's ~600-652px search / ~680px AI column convention (rubric 5.5).
- Type: DDG uses its own sans (DDG Atlas/Proxima-like) at 400 body, 600-700
  headings; result titles ~18-19px medium, snippets ~14-15px. Nothing exotic.
- Accent: DDG orange (#de5833 family) is used almost exclusively for the brand mark
  and interactive highlights (the underlined "private", active tab underline is
  blue-purple in dark mode); the AI surfaces use a cool blue/violet for the model
  and Search Assist identity. Orange signals brand, not AI. Dark mode palette is
  notable: a true near-black neutral scale with elevated card surfaces a step
  lighter (#282828-ish cards on #1c1c1c-ish page), muted grey text hierarchy, and
  saturated result titles in a soft periwinkle. cauce's dark mode should target the
  same "elevated surface step" model rather than inverting hues.
- Result card anatomy (classic): favicon + full URL line (the URL is shown in full,
  unlike Google's breadcrumb), title link in accent color, two-line snippet, no
  border/shadow per result, whitespace separation only. Matches
  `patterns-result-list.md` exactly; DDG is one of the convergent sources.
- Privacy banner placement: top-of-thread full-width bar (Duck.ai) and "Private"
  badge in the SERP filter row (classic). Two placements, one idea: privacy is
  ambient chrome, not a modal.

## Verdict for cauce

1. Keep the citation bar higher than DDG's. Search Assist's end-of-paragraph domain
   chips and Duck.ai's loose footnotes are the weakest citation treatment among
   major AI search surfaces; cauce's per-claim `[n]` markers linked to source cards
   are a real differentiator, and the DDG backend supplies real URLs to back them.
2. Add a "stale cache on throttle" state. Because cauce's backend is DDG, intermittent
   403/CAPTCHA throttling is a first-class failure mode. When search fails, offer
   the cached result set with an age notice instead of a bare error; record this in
   `patterns-states.md` as a cauce-specific state.
3. Copy DDG's privacy-chrome placement, not its sparseness. The persistent
   "Anonymized" banner at the top of the thread and the one-word underlined trust
   copy on landing are cheap, effective patterns cauce can mirror (e.g. a "served
   locally, cached on your machine" banner for AI mode and history).
4. Adopt DDG's motion floor. Zero animation on SERP render, motion budget spent
   only on streaming and expand/collapse. This is the conservative anchor for
   `patterns-motion.md` durations and validates rubric 7.6's anti-pattern list.
5. Mirror the always-visible model control. DDG keeps the current model as a
   labelled dropdown inside the composer with cost/reasoning annotations in the
   menu; cauce's model selector should show the active model inline per turn and
   annotate capability (context length, tool support) in the dropdown, not bury it
   in settings.

## Sources

Shots (`.agents/docs/screens/references/shots/`):

- `duckduckgo-classic-input-idle.png` (classic landing input)
- `duckduckgo-serp-search-assist-dark.png` (dark SERP, Search Assist card, result anatomy, sitelinks)
- `duckai-landing-idle.png`, `duckai-landing-typed-idle.png` (Duck.ai landing, composer with model dropdown and Fast toggle)
- `duckai-answer-complete-privacy-banner.png` (thread with privacy banner, model chip, message actions, pinned composer)

Web:

- https://duckduckgo.com/duckduckgo-help-pages/choosing-the-best-model-for-my-duckai-chat (model cost tiers, reasoning modes, usage limits)
- https://www.techradar.com/pro/duck-ai-review (interface minimalism, sidebar recents, local history, tier model)
- https://clickup.com/blog/duck-ai-vs-chatgpt/ (proxy architecture, mid-conversation model switching, no cross-chat memory, upload behavior)
- https://websites2know.com/duck-ai-review/ (rate-limit behavior, provider agreements)
- (Substack "Duck Tales: improving AI chat organization" was 404 at time of writing; chat-organization details taken from the reviews above instead.)
