# Screen: Search results (`/search?q=...&p=2`, `/?q=...`)

The primary surface for the two things humans do here: see instantly what
is cached, and share a cached search with an agent. One screen, two
modes, one url scheme:

1. **Search** (default): card-less Google-anatomy results, cache
   transparency meta line with a clickable `cached` badge, share row,
   letters pager (the word "oxe" as page links).
2. **AI** (`&mode=ai`, v0.3+): answer-first streaming view in the
   Morphic / Perplexity direction: the answer renders above a horizontal
   row of cited source cards, with related questions below.

`GET /search?q=...&p=2` is the canonical shareable results url
(back/forward works). Content negotiation: the same url serves HTML to
browsers and the Exa-shaped JSON payload to `Accept: application/json`
clients regardless of mode. The segmented Search / AI toggle sits in the
results-header pill so the same query can be re-run in the other mode
without retyping. When the AI backend is unavailable, `&mode=ai` urls
stay on Search results with a small inline notice and the disabled
segment communicates why (no redirect).

## ASCII mockup

### Mockup A: Search mode (`/search?q=python+asyncio`)

~652px content column, centered; whitespace separates results, no cards.

```
+------------------------------------------------------------------+
| oxe   search   [history]  dashboard   (?)  settings  [gh] v0.4.0 |
+------------------------------------------------------------------+
|   (  python asyncio                ( Search|AI )  ()  )          |
|                                                                  |
|  12 results - from cache - 3h old   [copy link]  [copy json]     |
|                                 ^ 'cached · 3h old' badge is     |
|                                   clickable: re-fetches from the |
|                                   web (refreshes this cache row) |
|  realpython.com                                                  |
|  Understanding asyncio - Real Python                             |
|  asyncio is Python's builtin library for writing concurrent      |
|  code using the async/await syntax, with event loops...          |
|  > cached page text preview                                      |
|                                                                  |
|  docs.python.org                                                 |
|  asyncio - Coroutine and concurrency documentation               |
|  documentation for the asyncio module: event loop, futures,      |
|  queues, synchronization primitives and subprocess...            |
|  > cached page text preview                                      |
|                                                                  |
|                 ← o x e →                                        |
|      ^ letters pager: the word 'oxe' repeating, one letter per   |
|        page; current page darker/bold; arrows at the edges       |
+------------------------------------------------------------------+
```

Result anatomy:

- favicon slot: `icons.duckduckgo.com/ip3/{domain}.ico`, 16px,
  lazy; absent on load failure, domain line still reads fine.
- domain line above the title (Google style), title link `#1a0dab`
  light / `#8ab4f8` dark, max two lines.
- two-line snippet, plain text, no border.
- `> cached page text preview`: collapsed `<details>` per result,
  expands to ~400 chars of cached page text, no extra requests.
- Meta line: `N results` plus source/age when served from cache; the
  `cached · <age>` badge is a button (tooltip: actually search the
  web) that re-runs the query against the network and refreshes the
  cache entry. `copy link` copies the current url; `copy json` copies
  the Exa payload.

### Mockup B: AI mode, streaming in progress (`&mode=ai`)

```
+------------------------------------------------------------------+
|   (  python asyncio                ( Search|AI )  ()  )          |
|   model [pick a model v]  (reasoning)   <- pill open in AI mode  |
|                                                                  |
|  ANSWER                                       [stop]  streaming..|
|                                                                  |
|  asyncio is Python's built-in library for concurrent code. It    |
|  provides the event loop, coroutines, and primitives like        |
|  TaskGroup and gather [1]. For CPU-bound work you typically      |
|  pair it with ProcessPoolExecutor [2]▌                           |
|                     ^ blinking block cursor marks the stream     |
|                                                                  |
|  SOURCES                                                         |
|  +-----------+  +-----------+  +-----------+  +-----------+     |
|  | [ ] real- |  | [ ] docs. |  | [ ] super-|  | ...more   |     |
|  | python.com|  | python.org|  | fastpython|  | streaming |     |
|  | Understan-|  | asyncio   |  | Async guide| | in...     |     |
|  | ding asyn-|  |           |  |           |  |           |     |
|  | cio   [1] |  |       [2] |  |       [3] |  |           |     |
|  +-----------+  +-----------+  +-----------+  +-----------+     |
|        ^ horizontal scroll row of compact cards; number badge    |
|          matches the inline citation; title + favicon + domain   |
+------------------------------------------------------------------+
```

### Mockup B2: AI mode, completed

```
+------------------------------------------------------------------+
|   (  python asyncio                ( Search|AI )  ()  )          |
|                                                                  |
|  ANSWER  [from cache]  3h old                 [view Search]      |
|                                                                  |
|  (full answer text, same anatomy as streaming but no cursor;     |
|   citation markers [1] [2] are links that scroll/highlight       |
|   the matching source card)                                      |
|                                                                  |
|  SOURCES                                                         |
|  +-----------+  +-----------+  +-----------+  +-----------+     |
|  | [1]       |  | [2]       |  | [3]       |  | [4]       |     |
|  +-----------+  +-----------+  +-----------+  +-----------+     |
|        <- card hover lifts it slightly; click opens new tab      |
|          and POSTs /click (same as Search-mode results)          |
|                                                                  |
|  RELATED                                                         |
|    - how does asyncio.gather differ from TaskGroup?              |
|    - is asyncio truly concurrent for CPU-bound work?             |
|    - how to run async code in Jupyter?                           |
|       ^ clicking one runs it as a new AI query (input updates)   |
+------------------------------------------------------------------+
```

### Mockup C: shared empty / error states

```
Search mode, 0 results:               Search mode, backend error:
|  no results                   |        |  error: search failed:  |
|                               |        |  502                    |
|  try: [ask AI instead]        |        |  [retry] [ask AI        |
|  (escape hatch shown only     |        |   instead]              |
|   when AI is available)       |        |                         |

AI, failed mid-stream:
|  ANSWER                                                    |
|  (partial answer kept on screen, cursor replaced by)       |
|  stream interrupted - [retry] [view Search]                |

AI, empty sources (answer impossible):
|  no sources found for this query - try fewer words, or     |
|  [view Search]                                             |
```

AI mode unavailable (backend off): `&mode=ai` urls do not redirect;
Search results render with a small muted notice (`AI mode is not
configured - set a model in settings`) under the pill and the AI
segment disabled in place.

## Behavior

### Shared

- Url contract: `/search?q=<query>&p=<page>` for Search mode,
  `&mode=ai` for AI. Navigation pushes state so back/forward and
  sharing work. `p` is absent on page 1 (canonical url strips it);
  urls above page 1 carry `p=N`.
- Mode toggle (segmented Search / AI inside the results-header pill)
  re-runs the same query in the other mode and rewrites the url
  (`&mode=ai` added / stripped). Toggle state mirrors `localStorage`
  (`oxe-mode`); the AI segment is visible but disabled when the AI
  backend is unavailable.
- Content negotiation unchanged: `Accept: application/json` on either
  url returns the Exa-shaped payload. AI mode additionally exposes the
  answer text in the HTML only; the JSON contract stays search-only.
- Suggestions: the results-header pill shares the landing dropdown
  (landing.md State 5): local history matches first, debounced
  DDG ac only when enabled (`oxe-ac` in `localStorage`).
  Enter selects and submits in the active mode, tab fills the input
  without submitting, escape closes. Anchoring and keyboard map are
  identical to the landing behavior; both inputs share `<SearchBox>`.
- Result / source-card click opens a new tab and POSTs `/click`
  (query hash, url, title, source) so it lands in `/history`.
- Cache transparency is mandatory in both modes: the Search meta line
  shows count plus the `cached · <age>` badge on cache hits (clicking
  it re-fetches from the web and refreshes the cache entry); the AI
  view shows a `from cache` badge with answer age on replay. AI answers
  are cached with the same TTL machinery as search results.

### Search mode

- Meta line (`N results`, cached badge on hits), copy link / copy json
  share row, `<details>` previews and loading dots as specified above.
- `copy json` copies the identical Exa payload for the query, same
  contract as `POST /search`.
- Pager: the word `oxe` rendered letter by letter, one letter per page,
  repeating for pages beyond 3 (`o x e o x ...`), capped at 10 pages.
  Current page is darker/bold with `aria-current`; the others are muted
  links. Prev/next arrows at the edges (prev only from page 2, next up
  to the cap). Page links carry `p=N`; page-1 links strip `p`.

### AI

- Submit in AI mode streams the answer: text appends in order with a
  blinking block cursor (`▌`) at the stream head. Streaming comes from
  the server over a chunked response, rendered progressively.
  `[stop]` aborts and keeps partial text with a "stopped" note.
- Inline citation markers `[n]` render as small superscript links.
  Clicking one scrolls to and briefly highlights the matching source
  card. Cards are compact horizontal tiles: favicon, domain, truncated
  title, number badge; domains truncate with an ellipsis
  (`realpython…`) rather than hyphen-breaking mid-word; the row
  scrolls horizontally when >4 cards.
- Related questions: server-suggested follow-ups rendered as plain
  list rows below sources. Clicking one pushes a new
  `?q=...&mode=ai` entry (no full page reload via JS).
- Progress without JS: no streaming; the server renders the completed
  answer directly (chunked transfer degrades to a slow single page).
  (CSR client today: the SPA owns all rendering.)
- Errors mid-stream keep partial text and offer retry / view Search;
  empty-source and zero-result queries offer the same escape hatch
  (`ask AI instead` / `view Search` buttons; the AI escape hatch only
  when AI is available).
- AI tool steps (if the backend emits them) render as compact rows
  above the answer; a `from cache` badge marks replays.

## Responsive

- Both modes share the ~652px centered column; the source-card row is
  the exception and runs full column width, scrolling horizontally
  with snap points on touch.
- Below ~700px the header input and toggle stack; snippets clamp to
  two lines at every width; related-question rows wrap freely.
- Source cards shrink to a fixed ~140px tile on narrow screens; the
  horizontal scroll is intentional, nothing else scrolls sideways.

## Notes

- Search-mode anatomy rationale carried over unchanged: favicon +
  domain above title, `#1a0dab` / `#8ab4f8` title, two-line snippet,
  whitespace-only separation; boxed-card look intentionally abandoned.
- AI direction grounded on miurla/morphic (streaming answer, related
  questions), ItzCrazyKns/Vane (answer engine layout), and the Mobbin
  Perplexity/Exa patterns (answer-first hierarchy, compact horizontal
  source cards, `/tmp/mobbin-imgs/` reference set, fetched 2026-09-14).
- Design tokens: near-black warm dark canvas (#0d0817-ish), off-white
  light mode (#fafafa), 15-16px body, dimmed 12-13px meta gray
  (#6b7280-ish family), title-link accent is the only saturated color.
  The cursor and source-card number badges reuse the accent at reduced
  opacity.
- AI answer generation is a server concern; this spec only pins the
  render contract: ordered text chunks, citation marker positions,
  source-card list, related-questions list. Stdlib-only server means
  the stream is chunked HTTP, not websockets.
- Register as a browser search engine with
  `https://search.localhost/?q=%s` (Search mode); an AI-mode engine
  entry can use `https://search.localhost/?q=%s&mode=ai`.
- Vite emits hashed asset filenames, so stale clients self-heal after
  upgrades.

## User flow checkpoints

Search mode:

```
landing -> submit -> /search?q=... loading (busy submit glyph)
   -> results render (meta line proves cache/network + age)
   -> follow-up: edit header input (suggestions dropdown: history /
      optional ac -> down/enter or tab to fill) then submit, page via
      pager, or toggle to AI for the same query
   -> click result -> new tab + POST /click -> /history shows it
   -> browser back returns to previous paged state
```

AI:

```
landing (AI mode) or [ask AI] toggle -> submit
   -> streaming state: partial answer + cursor + [stop],
      source cards fill in as they resolve
   -> completed state: full answer, meta line, related questions
   -> follow-up: click a related question (new AI query) or
      [view Search] to re-run in Search mode
   -> click a source card -> new tab + POST /click -> /history
   -> back / forward restores either the completed answer (cache)
      or re-streams if the answer expired past ttl
```
