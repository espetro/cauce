# User flow checkpoints + URL state contract

Key states a user (or QA agent) can drive the oxe web UI into, and how each
can be reached via URL so a QA agent can deep-link directly. Verified against
the running app (vite dev + `uv run oxe`, 2026-09-15) with playwright; code
references are to `ui/src`.

## State management recommendation

**Custom hooks + preact-iso (`useLocation().query` / `route()`), zero new deps.**

Rationale:

- All UI state worth deep-linking is shallow (3-5 params max, one level, no
  nesting) and is already derived from the URL: `search.tsx` reads
  `query?.mode`, navigates with `route()`. There is no state graph that
  justifies a store library.
- `qss` (+2KB) solves URL-string building/parsing that `URLSearchParams`
  already solves in-platform. `@preact/signals` (+1.5KB + adapters) and
  nanostores (+~800B + bindings) shine for cross-component shared state;
  here the only truly global bits (mode, settings-open) are single
  booleans/strings that belong in the URL for shareability anyway.
- Budget is 40KB gz JS. Every KB not spent on state plumbing is spent on the
  answer stream and markdown renderer, which are the heavy parts.

Pattern: a tiny `useUrlState(key, default)` hook on top of
`useLocation()` from `preact-iso` (see `ui/src/routes/search.tsx:18-44` for
the existing hand-rolled version of exactly this). Param writes use
`route(path + "?" + new URLSearchParams(...), true)`.

## URL param contract

Canonical base URLs are `/search` (results), `/history`, `/dashboard`, `/`.

| Param | Values | Checkpoint(s) | Status |
|---|---|---|---|
| `q` | query text | search results, AI answer, cached-hit, error states | **works today** |
| `p` | page number, 1-based | (none) | **deprecated**: continuous scroll replaced the pager. Deep links with `p` are accepted but ignored (stripped from the url); generated links never carry it. Pagination is no longer URL-addressable |
| `mode` | `ai` (absent = Search) | AI streaming / AI done / AI unavailable | **works today**; canonical url rewrite keeps the param in sync with the in-pill toggle. Unavailable AI + `mode=ai` stays on Search results with an inline notice (no redirect) |
| `settings` | `open` \| `close` (absent = closed) | settings dialog open on any route | **works today**: header reads `?settings=open`; close/save strips the param |
| `since` | `24` \| `168` \| `720` \| `all` | history time-filtered | **planned**: filter is client state only (`routes/history.tsx`), URL params ignored on load |
| `qf` | query-text filter substring | history text-filtered | **planned**: same |
| `suggest` | `1` (+ required `q`) | suggestions dropdown open | **planned, QA-only**: dropdown open state is internal (`SearchBox.tsx`) |
| `force` | `error` \| `ai-off` \| `empty` | backend-error state, AI-unavailable notice, empty results | **planned, QA-only**: stubs the fetch layer; without it error states are only reachable by killing the backend |

Non-param checkpoints (no URL state needed or possible):

- **Cached hit**: any `/search?q=<already-searched>` after TTL-relevant wait.
  Addressable by URL already; QA should search once, invalidate nothing,
  reload same URL, and assert the `cached · <age>` badge (clicking it
  re-fetches from the web).
- **Streaming-in-progress**: transient; deep-linking `mode=ai` lands there
  naturally for uncached queries, then completes. Pin the *mid-stream* state
  only by throttling in DevTools, not via URL.
- **Landing idle / landing AI mode**: `/` ; mode comes from `localStorage`
  (`oxe-mode`), deliberately not a URL param. Landing AI mode also shows
  the in-pill second row (model combobox + reasoning chip).

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
  unavailable).
- URL: `/` — mode lives in `localStorage.oxe-mode`, not URL. Addressable
  only after one interaction or by presetting localStorage in the QA
  harness.
- Assert: pill morphs open with a second row: model combobox (filter
  input + listbox) + `reasoning` toggle chip; placeholder becomes
  `Ask anything privately`.

### 3. Search classic, results page 1
- Reach: submit query in Search mode.
- URL: **`/search?q=<query>`** — works today (no `p` param on page 1).
  Verified live: results list with meta line (`N results`) and the
  `cached · <age>` badge on cache hits.

### 4. Search classic, continuous scroll
- Reach: submit a query, then scroll near the end of the loaded results.
- URL: **`/search?q=<query>`** (no `p` param; the old `p=N` deep links are
  ignored and stripped). Further pages (backend caps at 10) are appended
  into a virtualized list with a subtle loading indicator at the end; a
  failed next-page fetch stops auto-loading with an inline retry, and the
  terminal state shows `end of results`. A `more results` button remains
  as the keyboard/no-scroll fallback.

### 5. Search loading / submitting
- Reach: transient between submit and render.
- URL: same as #3; busy glyph replaces the submit icon (`search.tsx:86`).
- QA: assert busy state via DOM within the in-flight window; not pin-able.

### 6. Suggestions dropdown open
- Reach: type >=2 chars in the landing or results-header pill input with
  matches.
- URL today: none. **Planned: `/search?suggest=1&q=pyth`** (or
  `/?suggest=1&q=pyth`): pre-fills the input with `q`, forces dropdown
  open. QA/dev-only; strip on submit.

### 7. Search classic, zero results
- Reach: query with no DDG hits, or `force=empty` (planned).
- URL: `/search?q=<junk-query>` works today (real empty query), or
  **planned `/search?q=<any>&force=empty`** to pin it deterministically.
- Assert: `no results` + `ask AI instead` escape hatch (only when AI is
  available).

### 8. Search classic, backend error
- Reach: backend down / 5xx.
- URL: `/search?q=<query>` when backend is down (`error: ...` with
  `retry`, plus `ask AI instead` when AI is available). Deterministic
  pinning: **planned `?force=error`** stubbing the fetch layer.

### 9. AI answer, streaming
- Reach: submit in AI mode with an uncached query.
- URL: **`/search?q=<query>&mode=ai`** — works today. Transient state:
  partial answer + cursor + `[stop]`, sources fill in. Verified: deep link
  lands in AI view (stream errored in sandbox but mode/navigation stuck).

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
  model in settings` (no redirect). Deterministic pin: **planned
  `?force=ai-off`** stubbing the availability check.

### 13. AI stream failed mid-stream
- Reach: stream errors partway (partial text kept, `retry` /
  `view Search`).
- URL: `/search?q=<query>&mode=ai` with backend failing mid-request; pin
  with `force=error` (planned).

### 14. AI empty sources
- Reach: AI run with no sources found.
- URL: `/search?q=<query>&mode=ai` on a sourceless query; pin with
  `force=empty`.

### 15. History, unfiltered
- Reach: nav to `/history`.
- URL: **`/history`** — addressable today. Assert stats line + newest-first
  rows + empty state when 0 clicks.

### 16. History, time-filtered
- Reach: select "last 24h" in the filter select.
- URL today: none (client state; `since=24` in URL is ignored on load).
  **Planned contract: `/history?since=24`** (also `168`=week,
  `720`=month, `all`=all time). Implementation: initialize `since` state
  from `useLocation().query`, write back on change (same pattern as mode
  in `search.tsx`).

### 17. History, query-text filtered
- Reach: type in the query-filter input.
- URL: **planned `/history?qf=<substring>`** (`q` is taken by the backend's
  history endpoint semantics; `qf` keeps UI-filter distinct).

### 18. Settings dialog open
- Reach: click `settings` in the header (any route).
- URL: **`/?settings=open`** (also `/search?q=x&settings=open` etc.) —
  **works today**. On close or save the param is stripped. Re-deep-linking
  `?settings=open` restores the open dialog.
- Assert: dialog with `ai` fieldset (provider select, model combobox
  with filter input, api key, base url, enabled toggle) and `theme`
  fieldset (System / Light / Dark segmented control, persisted in
  `localStorage.oxe-theme`).

### 19. Settings saved
- Reach: submit the settings form.
- URL: none; persisted backend-side via `PUT /settings` (theme in
  `localStorage`). After save: `saved` state on the button, the dialog
  auto-closes, model lists refetch (`bumpModels()`), and the param is
  stripped. QA verifies by re-opening #18 and asserting values, and by
  `/health` reflecting the change where applicable.

### 20. Dashboard
- Reach: nav to `/dashboard`.
- URL: **`/dashboard`** — addressable today. Live cache-stats panel
  (`GET /cache/stats`) plus placeholder panels for log-derived data;
  no params planned (spec: window is build-time constant).

## QA agent convention

1. Deep-link: drive any checkpoint above via its URL; don't click-through
   when a URL exists.
2. Discover: when you find a new meaningful state while testing, name it,
   add it here with its URL recipe, and implement (or propose) the param
   that pins it, then update the "URL param contract" table and the
   "URL state & QA checkpoints" section of `ui/AGENTS.md`.
3. `force=*` and `suggest=1` params are QA/dev-only: they must never
   alter normal user flows and should be inert in production builds if
   they leak. They are planned but not implemented; do not assume they
   work.
4. Theme (System/Light/Dark) and mode persist in `localStorage`
   (`oxe-theme`, `oxe-mode`); QA harnesses may preset them, they are
   not URL-addressable.
