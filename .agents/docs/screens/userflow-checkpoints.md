# User flow checkpoints + URL state contract

Key states a user (or QA agent) can drive the oxe web UI into, and how each
can be reached via URL so a QA agent can deep-link directly. Carried over
from the legacy (`v0.4.0`) UI spec as the behavioral source of truth for the
v0.5.0 rebuild. **Unverified against the new build**: the screens described
here do not exist yet on `main`. Wave 3 re-verifies each checkpoint against
the rebuilt screens as it lands, one screen at a time.

## State management

See `ui/AGENTS.md` for the state ladder (URL first, then other non-opaque
state, then `useReducer` FSM, then `@xstate/store`, capped at three stores).
This file only records which checkpoints are URL-addressable today versus
planned; it does not prescribe an implementation.

## URL param contract

Canonical base URLs are `/search` (results), `/history`, `/dashboard`, `/`.

The param-by-param contract will be a generated table sourced from each
route's `validateSearch` valibot schema once those schemas exist (wave 3
builds the routes). Until then, treat the per-checkpoint URL recipes below
as the working contract; this file does not hand-maintain a second copy of
the schema.

Non-param checkpoints (no URL state needed, or not URL-addressable today):

- **Cached hit**: any `/search?q=<already-searched>` after TTL-relevant wait.
  Addressable by URL already; QA should search once, invalidate nothing,
  reload same URL, and assert the `cached · <age>` badge (clicking it
  re-fetches from the web).
- **Streaming-in-progress**: transient; deep-linking `mode=ai` lands there
  naturally for uncached queries, then completes. Pin the *mid-stream* state
  only by throttling in DevTools, not via URL.
- **Landing idle**: `/`, addressable today, no params.
- **Theme** (System / Light / Dark): stays in `localStorage` (rung 2 of the
  state ladder), not the URL — it is cross-session UI preference, not a
  shareable result. Requirement: a rung-2 valibot codec for the
  `localStorage` value (does not exist yet; needed so a stale stored shape
  from a prior release fails closed instead of throwing).

## Checkpoints

### 1. Landing idle
- Reach: open `/`.
- URL: `/` — addressable today. No params.
- Assert: wordmark + tagline (`your local web intel layer`) + 672px pill
  centered (~42-45% vh), input autofocused, in-pill `Search | AI`
  segmented toggle, no meta line. Navbar shows (?) about hint, settings,
  GitHub icon.

### 2. Landing, AI mode selected
- Reach: click AI segment in the pill (visible but disabled when AI is
  unavailable), or deep-link.
- URL: **`/?mode=ai`**. Per the state ladder's rung 1 (anything reachable
  by link belongs in the URL, not `localStorage`), `mode` becomes a
  `validateSearch` param on the landing route as well as on `/search` —
  this is a requirement for the rebuild, not yet implemented.
- Assert: pill morphs open with a second row: model combobox (filter
  input + listbox) + `reasoning` toggle chip; placeholder becomes
  `Ask anything privately`.

### 3. Search classic, results page 1
- Reach: submit query in Search mode.
- URL: **`/search?q=<query>`** — works today. Verified live: results list
  with meta line (`N results`) and the `cached · <age>` badge on cache
  hits.

### 4. Search classic, continuous scroll
- Reach: submit a query, then scroll near the end of the loaded results.
- URL: **`/search?q=<query>`** (no page param; pagination is not
  URL-addressable). Further pages (backend caps at 10) are appended into
  a virtualized list with a subtle loading indicator at the end; a failed
  next-page fetch stops auto-loading with an inline retry, and the
  terminal state shows `end of results`. A `more results` button remains
  as the keyboard/no-scroll fallback.

### 5. Search loading / submitting
- Reach: transient between submit and render.
- URL: same as #3; busy glyph replaces the submit icon.
- QA: assert busy state via DOM within the in-flight window; not pin-able.

### 6. Suggestions dropdown open
- Reach: type >=2 chars in the landing or results-header pill input with
  matches.
- URL today: none. Scheduled: `/search?suggest=1&q=pyth` (or
  `/?suggest=1&q=pyth`): pre-fills the input with `q`, forces dropdown
  open. QA/dev-only; strip on submit. Reachable once wave 2 builds the
  `suggest=1` fixture (wave 2 step 17).
- Assert: dropdown open, matches listed, keyboard nav works.

### 7. Search classic, zero results
- Reach: query with no DDG hits, or `force=empty` once built.
- URL: `/search?q=<junk-query>` works today (real empty query); once wave 2
  lands the `force` fixture, `/search?q=<any>&force=empty` pins it
  deterministically (wave 2 step 17).
- Assert: `no results` + `ask AI instead` escape hatch (only when AI is
  available).

### 8. Search classic, backend error
- Reach: backend down / 5xx.
- URL: `/search?q=<query>` when backend is down (`error: ...` with
  `retry`, plus `ask AI instead` when AI is available). Once wave 2 lands
  the `force` fixture, `?force=error` pins this deterministically (wave 2
  step 17).

### 9. AI answer, streaming
- Reach: submit in AI mode with an uncached query.
- URL: **`/search?q=<query>&mode=ai`** — works today. Transient state:
  partial answer + cursor + `[stop]`, sources fill in.

### 10. AI answer, done with sources
- Reach: stream completes, or re-open a cached answer.
- URL: **`/search?q=<query>&mode=ai`** — same URL, terminal state. Assert:
  full answer, citation markers, source card row, related questions,
  `view Search` toggle; `from cache` badge on replay.

### 11. AI answer, cached hit
- Reach: re-open #10's URL after completion.
- URL: same. Assert meta line shows `from cache` + answer age.

### 12. AI mode unavailable
- Reach: `/v1/models` reports unavailable; the AI segment disables in
  place (visible, dimmed, tooltip) both on landing and results; landing
  falls back to Search mode.
- URL: `/search?q=<query>&mode=ai` with the AI backend off stays on
  Search results with the notice `AI mode is not configured - set a
  model in settings` (no redirect).

### 13. AI stream failed mid-stream
- Reach: stream errors partway (partial text kept, `retry` /
  `view Search`).
- URL: `/search?q=<query>&mode=ai` with backend failing mid-request; once
  wave 2 lands the `force` fixture, `force=error` pins it (wave 2 step 17).

### 14. AI empty sources
- Reach: AI run with no sources found.
- URL: `/search?q=<query>&mode=ai` on a sourceless query; once wave 2
  lands the `force` fixture, `force=empty` pins it (wave 2 step 17).

### 15. History, unfiltered
- Reach: nav to `/history`.
- URL: **`/history`** — addressable today. Assert stats line + newest-first
  rows + empty state when 0 clicks.

### 16. History, time-filtered
- Reach: select "last 24h" in the filter select, or deep-link.
- URL: **`/history?since=24`** — **works today** (also `168`=week,
  `720`=month; `all`=default, stripped from the url). Server-side filter
  against `GET /api/history`; the select writes back via router
  navigation (same pattern as `mode`).

### 17. History, query-text filtered
- Reach: type in the query-filter input, or deep-link.
- URL: **`/history?qf=<substring>`** — **works today** (`q` is taken by
  the backend's history endpoint semantics; `qf` keeps the UI filter
  distinct and maps to the server-side `q` substring param).

### 18. Settings dialog open
- Reach: click `settings` in the header (any route).
- URL: **`/?settings=open`** (also `/search?q=x&settings=open` etc.) —
  **works today**. On close or save the param is stripped. Re-deep-linking
  `?settings=open` restores the open dialog.
- Assert: dialog with `ai` fieldset (provider select, model combobox
  with filter input, api key, base url, enabled toggle) and `theme`
  fieldset (System / Light / Dark segmented control, persisted in
  `localStorage` behind the rung-2 valibot codec noted above).

### 19. Settings saved
- Reach: submit the settings form.
- URL: none; persisted backend-side via `PUT /settings` (theme in
  `localStorage`). After save: `saved` state on the button, the dialog
  auto-closes, model lists refetch, and the param is stripped. QA verifies
  by re-opening #18 and asserting values, and by `/health` reflecting the
  change where applicable.

### 20. Dashboard
- Reach: nav to `/dashboard`.
- URL: **`/dashboard`** — addressable today. Live stats panel
  (`GET /api/stats`) plus placeholder panels for log-derived data; no
  params planned (spec: window is build-time constant).

## QA agent convention

1. Deep-link: drive any checkpoint above via its URL; don't click-through
   when a URL exists.
2. Discover: when you find a new meaningful state while testing, name it,
   add it here with its URL recipe, and implement (or propose) the param
   that pins it, then update the "URL param contract" section and
   `ui/AGENTS.md`.
3. `force=*` and `suggest=1` params are QA/dev-only: they must never
   alter normal user flows and should be inert in production builds if
   they leak. They are scheduled for wave 2 (step 17), not yet
   implemented; checkpoints 6, 7, 8, 13 and 14 above become reachable by
   URL once that lands.
4. Theme (System/Light/Dark) and mode: mode is being moved to a URL
   param (rung 1, see checkpoint 2); theme stays in `localStorage` behind
   a rung-2 valibot codec. QA harnesses may preset the `localStorage`
   value for theme.
