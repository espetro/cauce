# Screen: Landing (`/`, no query)

The entry state: a quiet, centered hero — small wordmark, one-line
tagline (`your local web intel layer`), and a single oversized pill
search bar (max 672px) with the Search / AI segmented toggle built into
its right end. This is also the browser search engine entry point
(`https://search.localhost/?q=%s` renders the same page straight into
results). No cards, no marketing copy, no hint line: mode explanations
live in the navbar (?) about panel. Mode persists in `localStorage`
(`cauce-mode`); when AI is unavailable the AI segment stays visible but
disabled.

## ASCII mockup

State 1: landing, Search mode (default).

```
+------------------------------------------------------------------+
| cauce   [search]  history  dashboard        (?)  settings  [gh] v0.4.0 |
+------------------------------------------------------------------+
|                                                                  |
|                                                                  |
|                              cauce                                 |
|                 your local web intel layer                       |
|                                                                  |
|        +--------------------------------------------+            |
|        |  search the web...        ( Search|AI ) () |            |
|        +--------------------------------------------+            |
|                                                                  |
+------------------------------------------------------------------+
^ generous vertical rhythm: wordmark -> tagline -> pill, each with
  clear breathing room; the pill block sits around 42-45% viewport
  height. The segmented toggle (Search = magnifier, AI = sparkle)
  lives inside the pill at the right end; the round button is submit.
```

State 2: AI mode selected. The pill morphs: a hairline divider reveals
a second action row inside the pill with model picker + reasoning chip.

```
|        +--------------------------------------------+            |
|        |  ask anything privately   ( Search|AI ) () |            |
|        |  ---------------------------------------- |            |
|        |  model [pick a model v]      (reasoning)   |            |
|        +--------------------------------------------+            |
```

State 3: submitting (enter pressed, Search mode).

```
|        |  python asyncio              ( .. ) |                  |
|        ^ busy dots replace the submit glyph;                     |
|          url bar already shows /search?q=...                     |
```

Empty/error states do not exist on landing (the input cannot be "empty
result"); a submit with an empty input is a no-op. A backend failure
routes to the error state of the results view in the active mode
(owned by search.md's Mockup C; landing only forwards there).

State 4: submitting in AI mode. Busy glyph on submit while the answer
stream connects; the answer renders on the search route.

State 5: suggestions dropdown open (typing, input focused, >=2 chars).

```
|        +--------------------------------------------+            |
|        |  python asyn|             ( Search|AI ) () |            |
|        +----------------+---------------------------+            |
|        |  YOUR HISTORY                               |            |
|        |  python asyncio grep                        |            |
|        |  python async generators                    |            |
|        |  WEB SUGGESTIONS                            |            |
|        |  python asyncio tutorial                    |            |
|        |  python asyncio vs threading                |            |
|        |  ----------------------------------------  |            |
|        |  web suggestions          [toggle on]       |            |
|        +--------------------------------------------+            |
|        ^ first row of each group is a muted caps label, not an   |
|          option; the footer row toggles network completion.      |
```

## Behavior

- Suggestions (autocomplete): a dropdown under the landing pill,
  fed by two sources in order. (1) `your history`: prefix/substring
  matches from local `search_log`, including queries MCP agents ran
  for this user (the `exa_user_history` idea surfaces at typing time,
  so past research is reused instead of re-researched). Deduped,
  most-recent-first, capped at 3. (2) `web suggestions`: DuckDuckGo
  ac endpoint matches, capped at 4, shown only when enabled.
- Local-first default: local matches are instant (cache hit latency)
  and aligned with cauce's purpose of reusing past searches across
  agents and users, so `your history` leads. Preferred, not enforced:
  remote cache backends are a legitimate future option. DDG ac
  reaches the network per debounce tick, adding latency and
  dependency on reach, so it ships behind a small `ac: on/off`
  toggle at the bottom edge of the dropdown (persisted in
  `localStorage` key `cauce-ac`); it may default on if it demonstrably
  yields better suggestions. Network calls are debounced to 300ms
  with in-flight cancellation. The two groups keep their caps labels
  so the user can tell what came from history vs. the network.
- Keyboard: down/up moves through combined options (group labels are
  skipped), enter selects and submits immediately, tab or right-arrow
  fills the input without submitting (escape hatch for edits),
  escape closes and restores the typed text. `aria-autocomplete`,
  `role="listbox"`/`option` semantics; active index managed in the
  component.
- Zero visual weight: same width as the pill, canvas background with a
  hairline border, selected row tinted; no icons, no counters. Hides
  on blur or when input has < 2 chars.
- No-JS: no dropdown at all; the plain `GET` form from the no-JS
  fallback is untouched.
- The same dropdown attaches to the results-header pill (search.md
  Mockup A), anchored identically; both share `<SearchBox>`.
- Hero layout: pill max-width 672px, centered column; wordmark (5xl,
  semibold), muted small tagline, then the pill, each separated with
  generous vertical rhythm. Autofocus on load. Enter submits; clicking
  the toggle does not submit.
- Mode toggle: segmented control inside the pill at the right end,
  light track with the active segment as a raised white pill, two
  options labeled **Search** (magnifier icon) and **AI** (sparkle
  icon), `role="radiogroup"`, arrow keys switch segments. Default
  Search. Persisted in `localStorage` key `cauce-mode`. In AI mode the
  submit targets `/search?q=...&mode=ai`; Search targets
  `/search?q=...` (mode param absent keeps existing urls shareable).
- AI unavailable (`/v1/models` says so): the AI segment stays visible
  but disabled (dimmed, tooltip "configure a model in settings");
  arrow-key navigation skips it. It never hides.
- AI second row: selecting AI morphs the pill open with a hairline
  divider and a second action row: a filterable model combobox (see
  ModelPicker in search.md Behavior) plus a small `reasoning` toggle
  chip. Both persist in `localStorage` (`cauce-ai-model`,
  `cauce-ai-reasoning`); model falls back to the first listed model.
- Placeholder swaps with mode: `Search privately` vs
  `Ask anything privately`.
- Header nav matches the rest of the app (search / history /
  dashboard); on landing the `search` link is marked active with
  brackets (`[search]`). Right side: (?) about hint, settings,
  GitHub icon, version.
- Server-side, `GET /?q=...` from a browser search engine skips landing
  and renders the results view directly in the mode encoded in the url
  (no `mode` param = Search mode).

## Responsive

- Pill width: `min(672px, 100vw - 48px)`; centered column, nothing
  else on the canvas.
- The AI second row stacks its model picker and reasoning chip on
  narrow screens.
- Header collapses as on other screens: brand left, links wrap; the
  version label hides below ~640px.

## Notes

- Mobbin reference (Perplexity-style landing, saved set
  `/tmp/mobbin-imgs/`): oversized centered input, one accent control,
  answer-first framing for the AI option. Keep cauce's restraint: no
  suggested-question chips on landing v1; related questions live in the
  AI answer view instead.
- Landing is intentionally cache-transparent-free: no meta line until
  there is a query.
- Suggestions are cheap server-side: one `LIKE` query against the
  existing `search_log` table over a `GET /suggest?q=...` endpoint
  (stdlib-only, cached like everything else); the DDG ac call happens
  client-side only when enabled, keeping the server out of the
  suggestion-latency path.
- `GET /` also serves the HTML search UI contract (AGENTS.md); the mode
  toggle is purely client-side state plus the `mode` url param, so
  curl-able urls stay stable.

## Queued improvements

Harvested from `references/insights/`; not part of the conformance contract above.

- Composer morph: animate the same pill from its hero position to the results
  header in one motion. The pill lands at the top of the column, never bottom-pinned.
- Model picker annotations (context length, tool support) in the dropdown. Needs
  model metadata the backend does not expose yet.

## Design references

- [`references/tokens.md`](references/tokens.md) - canonical type scale,
  spacing, radii, color roles, motion durations behind everything on this
  screen.
- [`references/patterns-search-input.md`](references/patterns-search-input.md)
  - hero pill placement/sizing, placeholder copy, mode toggle placement,
  keyboard affordances, focus state, and the `## Hybrid (classic + AI)`
  section governing the shared `<SearchBox>` and suggestions dropdown.
- [`references/patterns-typography.md`](references/patterns-typography.md)
  - wordmark and tagline type scale.
- [`references/patterns-motion.md`](references/patterns-motion.md) -
  suggestions dropdown open/close motion.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - hero column placement (~42-45% viewport height), header anchoring,
  breakpoints.

## User flow checkpoints

```
entry (landing) -> choose mode -> type query (>=2 chars opens
   suggestions: history first, then DDG ac when enabled; down/enter
   selects+submits, tab fills without submitting) -> submit
   -> /search?q=...&mode=ai|<none> (loading state in that mode's view)
   -> results (Search mode) or streamed answer (AI)
   -> follow-up: edit query in the results header input, or
      toggle mode on the results page (same query re-runs in new mode)
   -> click a result / source card -> recorded in /history
   -> back: browser back returns to previous query state (pushState)
```
