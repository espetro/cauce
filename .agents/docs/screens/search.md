# Screen: Search results (`/search?q=...&p=2`, `/?q=...`)

The primary surface for the two things humans do here: see instantly what
is cached, and share a cached search with an agent. One screen, two
modes, one url scheme:

1. **Traditional** (default, existing behavior): card-less Google-anatomy
   results, cache transparency meta line, share row, pager.
2. **AI** (`&mode=ai`, v0.3+): answer-first streaming view in the
   Morphic / Perplexity direction: the answer renders above a horizontal
   row of cited source cards, with related questions below.

`GET /search?q=...&p=2` is the canonical shareable results url
(back/forward works). Content negotiation: the same url serves HTML to
browsers and the Exa-shaped JSON payload to `Accept: application/json`
clients regardless of mode. A mode toggle sits in the results header so
the same query can be re-run in the other mode without retyping.

## ASCII mockup

### Mockup A: traditional mode (`/search?q=python+asyncio`)

~652px content column, centered; whitespace separates results, no cards.

```
+------------------------------------------------------------------+
| oxe   search   history   cache   health   api            v0.3.x  |
+------------------------------------------------------------------+
|           (  python asyncio                          ) [search]  |
|           [ traditional | AI ]                                   |
|                                                                  |
|  12 results - from cache - 3h old - ttl 21h left   [sort: relevance] |
|  [copy link]  [copy json]                                        |
|                                                                  |
|  [ ] realpython.com                                              |
|      Understanding asyncio - Real Python                         |
|      asyncio is Python's builtin library for writing concurrent  |
|      code using the async/await syntax, with event loops...      |
|      > cached page text preview                                  |
|                                                                  |
|  [ ] docs.python.org                                             |
|      asyncio - Coroutine and concurrency documentation           |
|      documentation for the asyncio module: event loop, futures,  |
|      queues, synchronization primitives and subprocess...        |
|      > cached page text preview                                  |
|                                                                  |
|  < previous   page 1 of 2   next >                               |
+------------------------------------------------------------------+
```

Result anatomy:

- `[ ]` favicon slot: `icons.duckduckgo.com/ip3/{domain}.ico`, 16px,
  lazy; absent on load failure, domain line still reads fine.
- domain line above the title (Google style), title link `#1a0dab`
  light / `#8ab4f8` dark, max two lines.
- two-line snippet, plain text, no border.
- `> cached page text preview`: collapsed `<details>` per result,
  expands to ~400 chars of cached page text, no extra requests.

### Mockup B: AI mode, streaming in progress (`&mode=ai`)

```
+------------------------------------------------------------------+
|           (  python asyncio                          ) [search]  |
|           [ traditional | AI ]                                   |
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
|           (  python asyncio                          ) [search]  |
|           [ traditional | AI ]                                   |
|                                                                  |
|  ANSWER - from cache - 3h old - ttl 21h left    [view classic]   |
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
|          and POSTs /click (same as traditional results)          |
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
traditional, 0 results:                 traditional, backend error:
|  0 results - from cache -     |        |  error: backend rate     |
|  just now - ttl 5m left       |        |  limited, retry shortly  |
|                               |        |  (previous results stay  |
|  no results                   |        |   intact on failure)     |
|  try: [ask AI instead]        |        |  [retry]                 |

AI, failed mid-stream:
|  ANSWER                                                    |
|  (partial answer kept on screen, cursor replaced by)       |
|  stream interrupted - [retry] [switch to classic results]  |

AI, empty sources (answer impossible):
|  no sources found for this query - try fewer words, or     |
|  [switch to classic results]                               |
```

## Behavior

### Shared

- Url contract: `/search?q=<query>&p=<page>` for traditional,
  `&mode=ai` for AI. app.js `pushState`s every navigation so
  back/forward and sharing work; server re-renders any direct hit
  without JS.
- Mode toggle in the header re-runs the same query in the other mode
  (`[view classic]` on the AI page and `[ask AI]` on the traditional
  page are the same affordance). Toggle state mirrors `localStorage`.
- Content negotiation unchanged: `Accept: application/json` on either
  url returns the Exa-shaped payload. AI mode additionally exposes the
  answer text in the HTML only; the JSON contract stays search-only.
- Suggestions: the results-header input shares the landing dropdown
  (landing.md State 5): local `search_log` matches always, debounced
  DDG ac only when enabled (`oxe-ac` in `localStorage`; local is
  preferred for latency and reuse, not enforced).
  Enter selects and submits in the active mode, tab fills the input
  without submitting, escape closes. Anchoring and keyboard map are
  identical to the landing behavior; only the wiring (`data-suggest`
  on both inputs) is shared via app.js.
- Result / source-card click opens a new tab and POSTs `/click`
  (query hash, url, title, source) so it lands in `/history`.
- Cache transparency is mandatory in both modes: meta line shows
  count (traditional) or answer age (AI), cache/network source, age,
  ttl remaining. AI answers are cached with the same TTL machinery as
  search results, so a completed answer replays from cache with the
  meta line intact.

### Traditional

- Meta line, copy link / copy json share row, pager and `<details>`
  previews behave exactly as in the existing spec (unchanged).
- `copy json` copies the identical Exa payload for the query, same
  contract as `POST /search`.

### AI

- Submit in AI mode streams the answer: text appends in order with a
  blinking block cursor (`▌`) at the stream head. Streaming comes from
  the server over a chunked response; app.js renders progressively.
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
- Errors mid-stream keep partial text and offer retry / mode switch;
  empty-source and zero-result queries fall back to the shared empty
  state with the classic-results escape hatch.

## Responsive

- Both modes share the ~652px centered column; the source-card row is
  the exception and runs full column width, scrolling horizontally
  with snap points on touch.
- Below ~700px the header input and toggle stack; snippets clamp to
  two lines at every width; related-question rows wrap freely.
- Source cards shrink to a fixed ~140px tile on narrow screens; the
  horizontal scroll is intentional, nothing else scrolls sideways.

## Notes

- Traditional anatomy rationale carried over unchanged: favicon +
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
- The traditional view's former ai-overview stub (`[ask ai]` slot) is
  superseded by the header mode toggle; no collapsed stub card remains.
- Register as a browser search engine with
  `https://search.localhost/?q=%s` (traditional); an AI-mode engine
  entry can use `https://search.localhost/?q=%s&mode=ai`.
- `ASSET_VERSION` cache-busting on `/static/app.js` and `ui.css` keeps
  stale clients honest after upgrades.

## User flow checkpoints

Traditional:

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
      [view classic] to re-run in traditional mode
   -> click a source card -> new tab + POST /click -> /history
   -> back / forward restores either the completed answer (cache)
      or re-streams if the answer expired past ttl
```
