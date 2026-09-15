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
| `p` | page number, 1-based | search classic results page N | **works today** (route reads it; pager navigates) |
| `mode` | `ai` (absent = traditional) | AI streaming / AI done / AI unavailable | **works today** (`search.tsx:18`); recommend also accepting `mode=classic` explicitly so mode is always in the URL for QA |
| `since` | `24` \| `168` \| `720` \| `all` | history time-filtered | **needs work**: filter is client state only (`routes/history.tsx`), URL params ignored on load |
| `qf` | query-text filter substring | history text-filtered | **needs work**: same |
| `settings` | `1` | settings dialog open | **needs work**: dialog state is `useState` in `Header.tsx:34`; init from URL, remove param on close |
| `suggest` | `1` (+ required `q`) | suggestions dropdown open | **needs work**: dropdown open state is internal (`SearchBox.tsx:28`); QA-only override |
| `force` | `error` \| `ai-off` \| `empty` | backend-error state, AI-unavailable notice, empty results | **needs work**: QA-only param that stubs the fetch layer; without it error states are only reachable by killing the backend |

Non-param checkpoints (no URL state needed or possible):

- **Cached hit**: any `/search?q=<already-searched>` after TTL-relevant wait.
  Addressable by URL already; QA should search once, invalidate nothing,
  reload same URL, and assert the `from cache` meta line.
- **Streaming-in-progress**: transient; deep-linking `mode=ai` lands there
  naturally for uncached queries, then completes. Pin the *mid-stream* state
  only by throttling in DevTools, not via URL.
- **Landing idle / landing AI mode**: `/` ; mode comes from `localStorage`
  (`oxe-mode`), deliberately not a URL param. If QA needs to pin it, a future
  `?mode=ai` on `/` that sets the toggle (without navigating) would be the
  consistent extension, but low priority.

## Checkpoints

### 1. Landing idle
- Reach: open `/`.
- URL: `/` — addressable today. No params.
- Assert: centered input autofocused, toggle `[ TRADITIONAL | ai ]`, no meta line.

### 2. Landing, AI mode selected
- Reach: click AI toggle on landing.
- URL: `/` — mode lives in `localStorage.oxe-mode`, not URL. Addressable only
  after one interaction or by presetting localStorage in the QA harness.

### 3. Search classic, results page 1
- Reach: submit query in traditional mode.
- URL: **`/search?q=<query>`** — works today. Verified live: renders results
  list with meta line (`N results - from cache/network - age`).
- Note: `run()` currently ignores `p` (see `useSearch.ts` — `run(q, p)` called
  as `run(q, page)` but the hook signature takes only `q`); page param is
  route state but pagination behavior needs backend paging first.

### 4. Search classic, page N
- Reach: click `next` in pager.
- URL: **`/search?q=<query>&p=2`** — route reads `p`; URL is shareable today,
  but the fetch layer must honor it once paging ships end to end.

### 5. Search loading / submitting
- Reach: transient between submit and render.
- URL: same as #3; busy glyph replaces the submit icon (`search.tsx:86`).
- QA: assert busy state via DOM within the in-flight window; not pin-able.

### 6. Suggestions dropdown open
- Reach: type >=2 chars in the landing or results-header input with matches.
- URL today: none. **Proposed: `/search?suggest=1&q=pyth`** (or `/?suggest=1&q=pyth`):
  pre-fills the input with `q`, forces dropdown open. QA/dev-only; strip on
  submit. Implementation: `SearchBox` initializes `value`/`open` from the param.

### 7. Search classic, zero results
- Reach: query with no DDG hits, or `force=empty`.
- URL: `/search?q=<junk-query>` works today (real empty query), or
  **proposed `/search?q=<any>&force=empty`** to pin it deterministically.
- Assert: "no results" + `[ask AI]` escape hatch.

### 8. Search classic, backend error
- Reach: backend down / 5xx.
- URL: `/search?q=<query>` when backend is down (verified live: "error:
  search failed: 502" with `[retry]`). Deterministic pinning:
  **proposed `?force=error`** stubbing the fetch layer.

### 9. AI answer, streaming
- Reach: submit in AI mode with an uncached query.
- URL: **`/search?q=<query>&mode=ai`** — works today. Transient state:
  partial answer + cursor + `[stop]`, sources fill in. Verified: deep link
  lands in AI view (stream errored in sandbox but mode/navigation stuck).

### 10. AI answer, done with sources
- Reach: stream completes, or re-open a cached answer.
- URL: **`/search?q=<query>&mode=ai`** — same URL, terminal state. Assert:
  full answer, citation markers, source card row, related questions,
  `[view classic]` toggle.

### 11. AI answer, cached hit
- Reach: re-open #10's URL after completion.
- URL: same. Assert meta line shows `from cache` + answer age.

### 12. AI mode unavailable
- Reach: `useAiAvailable()` returns false; toggle disables/falls back
  (`routes/index.tsx:19`).
- URL: `/search?q=<query>&mode=ai` with the AI backend off. Deterministic
  pin: **proposed `?force=ai-off`** stubbing the availability check.

### 13. AI stream failed mid-stream
- Reach: stream errors partway (verified live: "stream interrupted - 502"
  with `[retry]` / `[switch to classic results]`, partial text kept).
- URL: `/search?q=<query>&mode=ai` with backend failing mid-request; pin
  with `force=error`.

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
- URL today: none (client state; `since=24` in URL is ignored on load —
  verified). **Proposed contract: `/history?since=24`** (also `168`=week,
  `720`=month, `all`=all time). Implementation: initialize `since` state
  from `useLocation().query`, write back on change (same pattern as mode in
  `search.tsx`).

### 17. History, query-text filtered
- Reach: type in the query-filter input.
- URL: **proposed `/history?qf=<substring>`** (`q` is taken by the backend's
  history endpoint semantics; `qf` keeps UI-filter distinct).

### 18. Settings dialog open
- Reach: click `settings` in the header.
- URL today: none (`useState` in `Header.tsx:34`). **Proposed:
  `/?settings=1`** (param valid on any path; the header is global).
  On save/close, the param is removed. Re-deep-linking `?settings=1`
  restores the open dialog with the saved values shown.

### 19. Settings saved
- Reach: submit the settings form.
- URL: none; persisted server-side/localStorage per `schema.ts`. QA
  verifies by re-opening #18 and asserting values, and by `/health`
  reflecting the change where applicable.

### 20. Dashboard
- Reach: nav to `/dashboard`.
- URL: **`/dashboard`** — addressable today. Static panels; no params
  planned (spec: window is build-time constant).

## QA agent convention

1. Deep-link: drive any checkpoint above via its URL; don't click-through
   when a URL exists.
2. Discover: when you find a new meaningful state while testing, name it,
   add it here with its URL recipe, and implement (or propose) the param
   that pins it, then update the "URL param contract" table and the
   "URL state & QA checkpoints" section of `ui/AGENTS.md`.
3. `force=*` params are QA/dev-only: they must never alter normal user
   flows and should be inert in production builds if they leak.
