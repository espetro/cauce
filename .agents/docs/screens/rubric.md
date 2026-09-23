# Screen review rubric

The checklist an evaluator subagent works from when reviewing a generated UI
screen against the reference docs and screen specs, per the generator/evaluator
loop in `~/UILOOP.md`. This rubric does not invent criteria: every row below
traces to a specific section in `.agents/docs/screens/references/*.md` or a
specific line/state in a screen spec (`.agents/docs/screens/{landing,search,
history,dashboard}.md`). If a criterion cannot be traced that way, it does not
belong here; raise it as a new reference-doc finding instead.

## How to use this rubric

1. Confirm the hard gates in `~/UILOOP.md` section 6 first (console errors,
   console warnings, network 5xx, axe critical/serious, layout collapse at 3
   viewports, e2e checkpoint). Any red there is an automatic FAIL; do not
   proceed to the rows below until all six are green.
2. Walk the section matching the screen under review (a landing review only
   needs Input, Typography, Layout, Motion, Accessibility; a search-mode
   review needs all eight).
3. Emit findings using `~/UILOOP.md` section 5's verdict schema:
   `{severity, area, expected, observed, fix}`. `expected` must cite the
   reference-doc section or screen-spec line named in this rubric's "Cites"
   column, not a restated paraphrase.
4. A P0 finding blocks the verdict at FAIL. P1/P2 findings are recorded but do
   not by themselves block a PASS; `~/UILOOP.md` section 6 note: hard-gate
   reds are the only automatic FAIL, but a full section of P1s on the same
   area should be treated as a P0-equivalent judgment call by the evaluator.

## 1. Input

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 1.1 | One `<SearchBox>` component serves both landing hero and results-header pill; no separate composer per mode | P0 | `references/patterns-search-input.md` ## Hybrid (classic + AI); `landing.md` Behavior "The same dropdown attaches to the results-header pill" |
| 1.2 | Pill height 48-56px desktop, touch target >= 44px, radius `rounded-full` (9999px) | P1 | `references/patterns-search-input.md` ## Sizing and shape; `references/tokens.md` ## Radii |
| 1.3 | Placeholder swaps with mode: `Search privately` (Search) vs `Ask anything privately` (AI) | P1 | `landing.md` Behavior "Placeholder swaps with mode"; `references/patterns-search-input.md` ## Placeholder text |
| 1.4 | Mode segmented control lives inside the pill, `role="radiogroup"`, arrow keys switch segments, Search is default | P0 | `landing.md` Behavior "Mode toggle"; `references/patterns-search-input.md` ## Mode toggles and attach controls |
| 1.5 | AI segment stays visible but disabled (dimmed, tooltip) when AI is unavailable; never hidden; arrow-key nav skips it | P0 | `landing.md` Behavior "AI unavailable"; `references/patterns-search-input.md` ## Hybrid (classic + AI) |
| 1.6 | Submit disabled at ~40% opacity until text is present; stop button replaces submit in the same slot during streaming | P1 | `references/patterns-search-input.md` ## Mode toggles and attach controls |
| 1.7 | Focus ring ~2px accent at ~40% alpha, border shifts to accent, transition 120-150ms | P2 | `references/patterns-search-input.md` ## Focus state |
| 1.8 | Suggestions dropdown: `your history` group first (capped 3), `web suggestions` group second (capped 4, DDG ac, debounced 300ms); group labels are non-selectable muted caps rows | P1 | `landing.md` ASCII mockup State 5; `landing.md` Behavior "Suggestions (autocomplete)" |
| 1.9 | Keyboard map: down/up moves through options skipping group labels, enter selects+submits, tab/right-arrow fills without submitting, escape closes and restores typed text; `aria-autocomplete`, `role="listbox"`/`option` | P0 | `landing.md` Behavior "Keyboard" |
| 1.10 | Toggling mode re-runs the identical query and rewrites the url (`&mode=ai` added/stripped) rather than clearing the input | P0 | `search.md` Behavior "Shared" "Mode toggle"; `references/patterns-search-input.md` ## Hybrid (classic + AI) |

## 2. Result list (Search mode)

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 2.1 | Per-result anatomy in order: favicon (16px, lazy, graceful on failure) + domain line, then title link, then two-line snippet, then optional `> cached page text preview` details | P0 | `references/patterns-result-list.md` ## Per-result anatomy; `search.md` Mockup A "Result anatomy" |
| 2.2 | Title link is the only saturated-color text in the row: `#1a0dab` light / `#8ab4f8` dark, max two lines | P1 | `references/patterns-result-list.md` ## Per-result anatomy item 2; `search.md` Mockup A "Result anatomy" |
| 2.3 | No border, no shadow, no alternating background around individual results; separation is whitespace only | P0 | `references/patterns-result-list.md` ## Vertical rhythm; `references/patterns-layout-grid.md` note "boxed-card look intentionally abandoned" |
| 2.4 | Meta line (`N results`, `cached · <age>` badge, `copy link`/`copy json`) sits above the list, closer to the input than to the first result | P1 | `references/patterns-result-list.md` ## Result-list-level metadata; `search.md` Mockup A |
| 2.5 | `cached · <age>` badge is clickable and re-fetches from the network, refreshing the cache entry | P0 | `search.md` Behavior "Shared" "Cache transparency is mandatory"; `search.md` Mockup A meta line note |
| 2.6 | Continuous scroll appends further pages automatically near the end of the list; a `more results` button remains as keyboard/no-scroll fallback; terminal state shows `end of results` | P1 | `search.md` Behavior "Search mode" "Continuous scroll"; `userflow-checkpoints.md` #4 |
| 2.7 | Content column ~600-652px, left-anchored text (column itself may be centered mid-page, but favicon/title/snippet text is left-aligned, not centered) | P0 | `references/patterns-layout-grid.md` ## Content column widths per mode |

## 3. Answer surface (AI mode)

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 3.1 | Order of reveal: query echo (<100ms) -> skeleton in answer slot (<100ms, 2-3 grey lines) -> source cards fade in before answer text -> answer streams token-by-token with trailing cursor -> related questions render last | P0 | `references/patterns-answer-streaming.md` ## Order of reveal (critical) |
| 3.2 | A caret/cursor is always visible during an active stream; no caret reads as broken | P0 | `references/patterns-answer-streaming.md` ## Streaming contract; `search.md` Mockup B "blinking block cursor" |
| 3.3 | Stop button is active for the whole stream and backed by a real abort; after stop, partial text is kept with a "Generation stopped"/"stopped" label and a way to regenerate/retry | P0 | `references/patterns-answer-streaming.md` ## Streaming contract; `search.md` Behavior "AI" "[stop] aborts and keeps partial text" |
| 3.4 | Submit is locked during streaming and re-enables the instant the stream ends | P1 | `references/patterns-answer-streaming.md` ## Streaming contract |
| 3.5 | Inline citation markers `[n]` are typed into the answer text at the point of the claim (not appended chrome), and are links that scroll to/highlight the matching source card | P0 | `references/patterns-answer-streaming.md` ## Citations; `search.md` Mockup B2 |
| 3.6 | Source cards render as a horizontal-scrolling row (favicon + domain + truncated title + number badge), never interleaved with prose, domains ellipsis-truncate rather than hyphen-break | P1 | `search.md` Behavior "AI" "Cards are compact horizontal tiles"; `references/patterns-answer-streaming.md` ## Layout |
| 3.7 | Related questions render as a plain list below sources; clicking one pushes a new `?q=...&mode=ai` entry without a full page reload | P1 | `search.md` Mockup B2 "RELATED"; `search.md` Behavior "AI" "Related questions" |
| 3.8 | `from cache` badge with answer age shown on a cached replay | P1 | `search.md` Behavior "Shared" "Cache transparency is mandatory"; `search.md` Mockup B2 |
| 3.9 | Answer column max-width ~680px / ~68ch, left-aligned within a centered page | P1 | `references/patterns-answer-streaming.md` ## Layout; `references/patterns-layout-grid.md` ## Content column widths per mode |
| 3.10 | Streaming region announced with `aria-live="polite"` and `aria-busy="true"` while generating | P0 | `references/patterns-answer-streaming.md` ## Streaming contract |

## 4. Typography

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 4.1 | Type scale matches `references/tokens.md` ## Type scale exactly (display hero, heading large, heading, body/answer, label, caption, domain/mono meta); that file wins on any numeric disagreement | P0 | `references/tokens.md` ## Type scale; `references/patterns-typography.md` ## Scale |
| 4.2 | Weight ceiling 600 (semibold) for headings/emphasis; body text stays 400; nothing uses thin or black weights | P1 | `references/patterns-typography.md` ## Rules |
| 4.3 | Answer body / echoed query: query echo is LARGER than the answer body (20-28px/600 vs 15-16px/400) | P1 | `references/patterns-typography.md` ## Rules "Question/echoed query is LARGER" |
| 4.4 | Negative tracking (-0.01 to -0.02em) only on headings; body/small text stays normal tracking | P2 | `references/patterns-typography.md` ## Rules |
| 4.5 | Landing wordmark is `text-4xl md:text-5xl font-semibold` scale, tagline is muted/small | P1 | `landing.md` Behavior "Hero layout"; `references/tokens.md` ## Type scale "Display hero" |
| 4.6 | Source-card domain text: 11-12px, mono, uppercase, tracking-wide, reduced opacity | P2 | `references/patterns-typography.md` ## daisyUI mapping for cauce; `references/tokens.md` ## Type scale |

## 5. Layout

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 5.1 | Header is fixed/sticky-top on every screen, never scrolls out of view | P0 | `references/patterns-layout-grid.md` ## Page frame |
| 5.2 | No footer on search-result screens; disclaimer copy (if any) sits directly under the answer, not in a page footer | P2 | `references/patterns-layout-grid.md` ## Page frame |
| 5.3 | Landing pill sits at ~42-45% viewport height, not vertically centered at 50% | P1 | `references/patterns-layout-grid.md` ## Header anchoring inside the column; `landing.md` ASCII mockup caption |
| 5.4 | Results-header pill sits at the top of the content column and stays there; never re-centers mid-page | P1 | `references/patterns-layout-grid.md` ## Header anchoring inside the column |
| 5.5 | Search mode column ~600-652px; AI mode column ~680px/68ch; neither runs edge-to-edge at any viewport | P0 | `references/patterns-layout-grid.md` ## Content column widths per mode |
| 5.6 | Dashboard panel grid is `repeat(auto-fit, minmax(20rem, 1fr))`, collapsing to single column below ~700px | P1 | `dashboard.md` Responsive; `references/patterns-layout-grid.md` ## Gutters and breakpoints |
| 5.7 | Header input/toggle stack below ~700px on search; history url column hides below ~768px; version label hides below ~640px | P2 | `references/patterns-layout-grid.md` ## Gutters and breakpoints; `search.md` Responsive; `history.md` Responsive; `landing.md` Responsive |
| 5.8 | Source-card row is the one horizontal-scroll exception; nothing else scrolls sideways except history's table as a last resort on very narrow screens | P1 | `search.md` Responsive; `history.md` Responsive |

## 6. States

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 6.1 | Loading skeleton appears <100ms after submit, 2-3 grey rounded lines of varying width, shimmer/pulse cycle 300-700ms | P1 | `references/patterns-states.md` ## Loading |
| 6.2 | Multi-step AI work shows a progressing status line (Searching / Reading / Writing), not a static skeleton for the whole wait | P1 | `references/patterns-states.md` ## Loading; `references/patterns-answer-streaming.md` ## Phase model |
| 6.3 | Classic zero results: one line `no results` plus `ask AI instead` shown only when AI is available; no illustration, no retry framing | P0 | `references/patterns-states.md` ## Hybrid (classic + AI) "Classic zero results"; `search.md` Mockup C |
| 6.4 | Classic backend error: `error: search failed: <code>` plus `retry` and `ask AI instead` on the same row | P0 | `references/patterns-states.md` ## Hybrid (classic + AI) "Classic backend error"; `search.md` Mockup C |
| 6.5 | AI empty sources: `no sources found for this query - try fewer words, or [view Search]`, distinct from classic zero results | P0 | `references/patterns-states.md` ## Hybrid (classic + AI) "AI empty sources"; `search.md` Mockup C |
| 6.6 | AI mid-stream failure: partial answer text kept, cursor replaced by `stream interrupted - [retry] [view Search]` | P0 | `references/patterns-states.md` ## Hybrid (classic + AI) "AI mid-stream failure"; `search.md` Mockup C |
| 6.7 | Escape hatch asymmetry: `ask AI instead` only appears from classic states, `view Search` only from AI states | P1 | `references/patterns-states.md` ## Hybrid (classic + AI) "The shared escape hatch is asymmetric" |
| 6.8 | AI mode unavailable: Search results render normally with a muted notice under the pill (`AI mode is not configured - set a model in settings`); AI segment disables in place, no redirect | P0 | `references/patterns-states.md` ## Hybrid (classic + AI) "AI mode unavailable"; `search.md` "AI mode unavailable" note |
| 6.9 | History empty state: `no clicks yet — open a result from the search page.` with stats line reading `0 clicks in last 24h · 0 total · —` | P1 | `history.md` "Empty state"; `references/patterns-states.md` ## Applies to (history.md empty state) |
| 6.10 | Rate-limit / 429 shown as a calm, distinct treatment ("Rate limit reached, retrying in Ns") separate from generic network-error copy | P2 | `references/patterns-states.md` ## Error |
| 6.11 | Reserve space (min-height) for the skeleton so skeleton-to-text causes no layout shift | P1 | `references/patterns-states.md` ## Loading |

## 7. Motion

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 7.1 | Duration tokens match `references/tokens.md` ## Motion durations (instant 0, fast 120, standard 220, slow 360); stream cadence is per-token, never artificially paced slower than the real stream | P1 | `references/tokens.md` ## Motion durations; `references/patterns-motion.md` ## Duration tokens |
| 7.2 | Token streaming shows a trailing block cursor that blinks at ~530ms opacity-step cadence | P2 | `references/patterns-motion.md` ## Signature motions item 1, item 7 |
| 7.3 | Source-card hover-lift: border warms to accent + lift shadow, 150-220ms ease-out; resting shadow stays minimal | P2 | `references/patterns-motion.md` ## Signature motions item 3; `references/tokens.md` ## Elevation / depth |
| 7.4 | Sources collapse/expand animates height ~220ms with a chevron rotating 180deg over the same duration | P2 | `references/patterns-motion.md` ## Signature motions item 4 |
| 7.5 | `prefers-reduced-motion: reduce` collapses streaming to a single fade-in, stops cursor blink, drops all durations to 0-80ms, and makes shimmer static; product stays fully usable | P0 | `references/patterns-motion.md` ## Reduced motion |
| 7.6 | Anti-patterns absent: no fake typewriter pacing slower than the real stream, no auto-scroll while the user is reading earlier content, no removing the stop button mid-stream, no full-answer-in-one-flash | P0 | `references/patterns-motion.md` ## Anti-patterns |
| 7.7 | Theme (light/dark) transitions scope to color properties only (150-200ms ease), never a blanket `*` transition that interpolates layout | P1 | `references/patterns-motion.md` ## Theme transitions |

## 8. Accessibility

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 8.1 | axe critical/serious issues: 0 (hard gate, checked before this rubric per `~/UILOOP.md` section 6) | P0 | `~/UILOOP.md` ## 6. Hard gates |
| 8.2 | Suggestions dropdown exposes `aria-autocomplete`, `role="listbox"`/`option`, and a managed active index | P0 | `landing.md` Behavior "Keyboard" |
| 8.3 | Mode segmented control exposes `role="radiogroup"` with arrow-key navigation that skips the disabled AI segment | P0 | `landing.md` Behavior "Mode toggle"; "AI unavailable" |
| 8.4 | Streaming region carries `aria-live="polite"` and `aria-busy="true"` while generating | P0 | `references/patterns-answer-streaming.md` ## Streaming contract; `references/patterns-states.md` ## Accessibility |
| 8.5 | Each citation link carries `aria-label="Source N: example.com"` | P1 | `references/patterns-answer-streaming.md` ## Citations |
| 8.6 | Thinking/loading state lives in a polite live region; streamed text replaces it rather than a layout jump | P1 | `references/patterns-states.md` ## Accessibility |

## 9. Operator pages (history, cache, engines, audit/trace, settings)

Applies to the v3 pages specced 2026-09-23. Walk this section plus
Typography, Layout, Motion and Accessibility for those screens. The wave 2
settled inputs (`.agents/plans/v3/wave-2-ui-and-observability.md`) are
normative here and are cited directly.

| # | Assertion | Severity | Cites |
|---|---|---|---|
| 9.1 | Footer shows the full `request_id` of the render as selectable text; no abbreviation in visible text | P0 | wave-2 "Settled inputs" (request_id footer); `cache.md` / `history.md` / `engines.md` / `audit.md` / `settings.md` Behavior "Footer" |
| 9.2 | The HTML page is the API handler under `Accept: text/html`: same params, same defaults, same row set as the JSON route | P0 | wave-2 "Settled inputs" (same handlers); each spec's Behavior "Data path" |
| 9.3 | All visible copy is sourced from `strings.rs`; no literal English in templates | P1 | wave-2 "Settled inputs" (strings.rs); each spec's Behavior last bullet |
| 9.4 | Filter forms are plain GET forms and work with JS disabled; `clear` appears only while a filter is active | P1 | `history.md` Behavior "Filters"; `cache.md` Behavior "Filter"; `audit.md` Behavior "Filters" |
| 9.5 | Empty states are one sentence with an action, and filtered-empty copy names the active filter | P1 | `references/patterns-states.md` ## Empty state; each spec's "Empty state" mockup |
| 9.6 | Errors render inline where the action happened (`error: ... (<status>)`), never a toast | P1 | `references/patterns-states.md` ## Error; `cache.md` Behavior "payload"; `settings.md` Behavior "Save" |
| 9.7 | Destructive actions confirm first with copy that names what goes; `delete all` uses the `error` color role | P0 | `cache.md` Behavior "Deletes"; `settings.md` Behavior "Cache block"; `references/tokens.md` ## Color roles |
| 9.8 | History rows are searches (one per `search_log` row) with clicks nested; `source` reads `cached · <age>`, `cached · expired` or `network · t<n>` | P0 | `history.md` Behavior "One row per search", "Source column" |
| 9.9 | Breaker chip reads exactly `Closed`, `Open` (with `retries in Ns`) or `HalfOpen`, coloured success/error/warning | P0 | `engines.md` "Breaker chip states"; `references/tokens.md` ## Color roles |
| 9.10 | Engine test query calls `/api/search?q=&engines=<id>` and renders the shared result partial under the card | P0 | `engines.md` Behavior "test query"; wave-2 W2-05 Do |
| 9.11 | Trace page output equals the CLI timeline (same renderer), shown preformatted in the mono face | P0 | `audit.md` Behavior: trace "Same code path"; wave-2 W2-06 Do |
| 9.12 | Settings shows `${env:...}` templates verbatim and env-pinned fields disabled with `set by CAUCE_*` | P0 | `settings.md` Behavior "Templates", "Environment overrides"; wave-2 W2-07 Do |
| 9.13 | No horizontal page scroll at 390 px: tables hide columns or become cards per spec, preformatted and JSON blocks scroll inside themselves | P0 | wave-2 "Exit criteria" 2; each spec's Responsive |
| 9.14 | Header nav: primary `search · history · dashboard`, operator group `engines · cache · audit`, `settings`; operator group collapses below 700 px | P1 | `README.md` "Header nav"; `references/patterns-layout-grid.md` ## Gutters and breakpoints |

## Sources

- `~/UILOOP.md` (loop mechanics, hard gates, verdict schema this rubric feeds)
- `.agents/docs/screens/references/*.md` (all ten reference docs, cited per row above)
- `.agents/docs/screens/{landing,search,history,dashboard,cache,engines,audit,settings}.md` (screen specs, cited per row above)
- `.agents/plans/v3/wave-2-ui-and-observability.md` "Settled inputs" and "Exit criteria" (section 9)
- `.agents/docs/screens/userflow-checkpoints.md` (checkpoint numbering referenced in a few rows)
