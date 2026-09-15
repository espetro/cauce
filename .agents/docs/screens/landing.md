# Screen: Landing (`/`, no query)

The entry state: a quiet, centered hero with an oversized search input and
a two-mode toggle hint (traditional / AI). This is also the browser search
engine entry point (`https://search.localhost/?q=%s` renders the same
page straight into results). No cards, no marketing copy, one accent
surface. The mode toggle is the only new control versus the previous
landing; it defaults to traditional (the current behavior) and persists
the choice in `localStorage`.

## ASCII mockup

State 1: landing, traditional mode (default).

```
+------------------------------------------------------------------+
| oxe   [search]  history  cache  health  api              v0.3.x  |
+------------------------------------------------------------------+
|                                                                  |
|                                                                  |
|                                                                  |
|                              oxe                                 |
|                 search the web, locally cached                   |
|                                                                  |
|              +------------------------------------+              |
|              |  search the web...                 |              |
|              +------------------------------------+              |
|                                                                  |
| [ TRADITIONAL | ai ]      <- mode toggle (caps = active)         |
|                    ^ active side is the filled/caps side         |
|                                                                  |
|        hint under toggle:                                        |
|        traditional: classic link results, cache metadata         |
|        AI: streaming answer with cited sources                   |
|                                                                  |
+------------------------------------------------------------------+
```

State 2: AI mode selected (toggle flips, hint swaps, focus stays in input).

```
|                                                                  |
|              [ traditional | AI ]                                |
|                            ^ caps marks the active side          |
|                  AI: streaming answer with cited sources         |
|                                                                  |
+------------------------------------------------------------------+
```

State 3: submitting (enter pressed, traditional mode).

```
|              +------------------------------------+              |
|              |  python asyncio               [..] |              |
|              +------------------------------------+              |
|              ^ busy dots replace the [search] glyph;             |
|                url bar already shows /search?q=...               |
```

Empty/error states do not exist on landing (the input cannot be "empty
result"); a submit with an empty input is a no-op. A backend failure
routes to the error state of the results view in the active mode
(owned by search.md's Mockup C; landing only forwards there).

State 4: submitting in AI mode. Busy input glyph plus a pending-stream
hint instead of the traditional busy dots:

```
|              +------------------------------------+              |
|              |  python asyncio               [..] |              |
|              +------------------------------------+              |
|              ^ connecting: answer will stream in below           |
```

State 5: suggestions dropdown open (typing, input focused, >=2 chars).

```
|              +------------------------------------+              |
|              |  python asyn|                       |              |
|              +----------------+-------------------+              |
|              |  y o u r  h i s t o r y            |              |
|              |  python asyncio grep               |              |
|              |  python async generators           |              |
|              |  web suggestions                   |              |
|              |  python asyncio tutorial           |              |
|              |  python asyncio vs threading       |              |
|              +------------------------------------+              |
|              ^ dropdown floats over the hint line;  first row    |
|                of each group is its muted caps label, not an      |
|                option. 'web suggestions' group only appears when  |
|                network completion is enabled.                     |
```

## Behavior

- Suggestions (autocomplete): a dropdown under the landing input,
  fed by two sources in order. (1) `your history`: prefix/substring
  matches from local `search_log`, including queries MCP agents ran
  for this user (the `exa_user_history` idea surfaces at typing time,
  so past research is reused instead of re-researched). Deduped,
  most-recent-first, capped at 3. (2) `web suggestions`: DuckDuckGo
  ac endpoint matches, capped at 4, shown only when enabled.
- Local-first default: local matches are instant (cache hit latency)
  and aligned with oxe's purpose of reusing past searches across
  agents and users, so `your history` leads. Preferred, not enforced:
  remote cache backends are a legitimate future option. DDG ac
  reaches the network per debounce tick, adding latency and
  dependency on reach, so it ships behind a small `ac: on/off`
  toggle at the bottom edge of the dropdown (persisted in
  `localStorage` key `oxe-ac`); it may default on if it demonstrably
  yields better suggestions. Network calls are debounced to 300ms
  with in-flight cancellation. The two groups keep their caps labels
  so the user can tell what came from history vs. the network.
- Keyboard: down/up moves through combined options (group labels are
  skipped), enter selects and submits immediately, tab or right-arrow
  fills the input without submitting (escape hatch for edits),
  escape closes and restores the typed text. `aria-autocomplete`,
  `role="listbox"`/`option`, active-descendant managed by app.js.
- Zero visual weight: borderless, same width as the input, canvas
  background with a hairline border, selected row tinted; no icons,
  no counters. Hides on blur, scroll, or when input has < 2 chars.
- No-JS: no dropdown at all; the plain `GET` form from the no-JS
  fallback is untouched.
- The same dropdown attaches to the results-header input (search.md
  Mockup A), anchored identically, since both inputs share the
  `data-suggest` wiring in app.js.
- Single oversized input, centered vertically around 35-40% of the
  viewport height. Autofocus on load. Enter submits; clicking the toggle
  does not submit.
- Mode toggle: segmented control, two options, `role="radiogroup"`.
  Default `traditional`. Persisted in `localStorage` key `oxe-mode`.
  In AI mode the submit targets `/search?q=...&mode=ai`; traditional
  targets `/search?q=...` (mode param absent keeps existing urls
  shareable and byte-identical for agents).
- Toggle active state convention: in ASCII mockups, caps marks the
  active side (`[ TRADITIONAL | ai ]` = traditional active); in the UI
  the active segment is filled. Same bracket convention as the header
  nav elsewhere: brackets mark the active link (`[history]`).
- The hint line under the toggle swaps per mode and never exceeds one
  line. It is muted gray, 13px; the toggle itself is the only filled
  element on the page.
- No-JS: the form is a plain `GET` to `/search` with a hidden `mode`
  field; toggle is two submit-adjacent radio inputs. JS progressively
  pushes the url and swaps to fetch-render.
- Header nav matches the rest of the app; on landing the `search` link
  is marked active (current page).
- Server-side, `GET /?q=...` from a browser search engine skips landing
  and renders the results view directly in the mode encoded in the url
  (no `mode` param = traditional).

## Responsive

- Input width: `min(560px, 88vw)`; centered column, nothing else.
- Toggle stays under the input, same center; on very narrow screens the
  hint line may wrap to two lines.
- Header collapses as on other screens: brand left, links wrap.

## Notes

- Mobbin reference (Perplexity-style landing, saved set
  `/tmp/mobbin-imgs/`): oversized centered input, one accent control,
  answer-first framing for the AI option. Keep oxe's restraint: no
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

## User flow checkpoints

```
entry (landing) -> choose mode -> type query (>=2 chars opens
   suggestions: history first, then DDG ac when enabled; down/enter
   selects+submits, tab fills without submitting) -> submit
   -> /search?q=...&mode=ai|<none> (loading state in that mode's view)
   -> results (traditional) or streamed answer (AI)
   -> follow-up: edit query in the results header input, or
      toggle mode on the results page (same query re-runs in new mode)
   -> click a result / source card -> recorded in /history
   -> back: browser back returns to previous query state (pushState)
```
