# Screen: Search (`/search?q=...` and `/?q=...`)

The primary surface for the two things humans do here: see instantly what
is cached, and share a cached search with an agent. `GET /search?q=...&p=2`
is the canonical, shareable results URL (back/forward works). `GET /?q=...`
stays as the browser search engine entry point and renders the same view.
Results are server-rendered plain HTML, card-less (Google anatomy):
favicon + domain above the title, blue title, two-line snippet, no boxes.
Content negotiation: the same URL serves HTML to browsers and the
Exa-shaped JSON payload to `Accept: application/json` clients.

## ASCII mockup

State 1: landing (no query).

```
+------------------------------------------------------------------+
| oxe   search   history   cache   health   api            v0.1.x  |
+------------------------------------------------------------------+
|                                                                  |
|                                                                  |
|                    (  search the web...          )               |
|                                  [search]                        |
|                                                                  |
|   (search box is centered; app.js intercepts submit and          |
|    pushes /search?q=... into the url bar. results area is        |
|    empty here)                                                   |
|                                                                  |
+------------------------------------------------------------------+
```

State 2: results (`/search?q=python+asyncio`). ~652px content column,
centered; whitespace separates results, no cards.

```
+------------------------------------------------------------------+
| oxe   search   history   cache   health   api            v0.1.x  |
+------------------------------------------------------------------+
|           (  python asyncio                          ) [search]  |
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
|  [ ] example.com                                                 |
|      Async IO in Python: overview and deep dive                  |
|      coroutines, tasks and futures explained with runnable       |
|      examples for asyncio.gather, TaskGroup and...               |
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

Result anatomy, one at a time:

- `[ ]` is the favicon slot: `<img src="https://icons.duckduckgo.com/
  ip3/{domain}.ico" width="16" height="16" loading="lazy">`. No image
  tab anywhere.
- domain line above the title (Google style), then the title link
  (`#1a0dab` light / `#8ab4f8` dark), max two lines.
- two-line snippet, plain text, no border.
- `> cached page text preview` is a collapsed `<details>` per result
  (People-Also-Ask style progressive disclosure). Expanded shows the
  cached page text truncated to ~400 chars.

State 3: empty and error (same layout, results area swaps).

```
+------------------------------------------------------------------+
|           (  xyzzyquuxnomatch                        ) [search]  |
|                                                                  |
|  0 results - from cache - just now - ttl 5m left                 |
|                                                                  |
|  no results                                                      |
|                                                                  |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
|           (  python asyncio                          ) [search]  |
|                                                                  |
|  error: backend rate limited, retry in a moment                  |
|  (previous results are left intact if a later search fails)      |
|                                                                  |
+------------------------------------------------------------------+
```

State 4: ai overview stub (collapsed slot reserved above results).

```
|  + [ask ai]                                                    + |
|  |  (collapsed stub card; never auto-runs, no LLM backend yet) | |
|  +-------------------------------------------------------------+ |
|  (clicking [ask ai] would stream an answer here with source      |
|  chips; anatomy reserved now, activation post-v0.2)              |
```

## Behavior

- Results URL is `/search?q=<query>&p=<page>`. app.js pushes it via
  `history.pushState` so back/forward work and the url is shareable.
  Server re-renders on any direct hit to the url (no JS needed).
- Cache transparency meta line: count, cache/network source, result age
  and remaining ttl. Data already exists (`_source`, `_q_hash`, ttl
  machinery); this just surfaces it.
- Share row per search: `copy link` copies the canonical
  `/search?q=...` url; `copy json` copies the Exa-shaped payload for the
  same query (identical contract to `POST /search`). One tiny vanilla JS
  handler each, `navigator.clipboard`.
- Content negotiation on the same url: browsers get HTML;
  `Accept: application/json` clients get the Exa JSON. An agent can be
  pointed at a shared link and curl it directly.
- Result click opens in a new tab and POSTs `/click` so it lands in
  `/history` (title, url, query hash, source).
- `<details>` previews expand client-side from already-cached text; no
  extra requests.
- Empty state: `no results` (empty results get the short negative ttl,
  so a quick retry hits cache). Error state: backend failure shows the
  error line; JS keeps previous results. Loading: busy indicator on the
  submit button, no skeletons.

## Responsive

- The ~652px content column is centered and shrinks fluidly below
  ~700px viewport; snippets clamp to two lines at every width.
- Share row and meta line wrap to stacked lines on narrow screens;
  nothing horizontal-scrolls.
- Header nav collapses gracefully: links wrap, brand stays left,
  version label drops below on very narrow viewports.
- Favicons are 16px, lazy-loaded, and simply absent on load failure
  (domain line still reads fine without them).

## Notes

- Google-anatomy justification: research points at the classic serifless
  list look: favicon + domain above title, `#1a0dab` / `#8ab4f8` title,
  two-line snippet, whitespace-only separation. The earlier boxed-card
  mockup is intentionally abandoned.
- Mobbin reference set (fetched live, 2026-09-14, saved at
  `/tmp/mobbin-imgs/`): n8n template gallery
  (mobbin.com/screens/e24ca4ff) is the closest analog to a results
  page: centered hero search over a dark canvas, single-line result
  titles, muted 2-line snippets, whitespace-only separation, small
  meta line per result (author, age, free/paid) in near-gray. Its
  `Results (638)` count + right-aligned `Sort: Relevance` control maps
  directly to our meta line + future sort affordance. Savee search
  (mobbin.com/screens/aae12f50) shows the same anatomy in pure black:
  floating pill search box, `Showing results for '...'` echo line,
  nothing else on screen. Unity Learn (mobbin.com/screens/464c4893)
  confirms filter chips under the header pill and a `Showing 61
  Results` echo as the pattern users already read fluently.
- Design tokens distilled from the reference set: dark canvas is a
  near-black with slight warmth (n8n's #0d0817-ish), NOT pure #000;
  results read at 15-16px with 1.5 line height; per-result meta at
  12-13px in a dimmed gray (#6b7280-ish); the only accent color is the
  title link; everything else is one hue stepped by opacity. Light
  mode inverts to off-white (#fafafa) per the /tmp/modern-web-ui doc,
  never pure white.
- Register as a browser search engine with
  `https://search.localhost/?q=%s`; the `/?q=` entry renders the same
  view as `/search?q=`.
- AI overview stub (trigger button, stream area, source chips) is
  future / post-v0.2: layout reserves the slot, no LLM backend yet, and
  it never auto-runs.
- `ASSET_VERSION` cache-busting on `/static/app.js` and `ui.css` keeps
  stale clients honest after upgrades.
- Result ordering and field rendering mirror the Exa-compatible JSON
  contract, so the HTML view, the shared JSON link, and MCP/HTTP clients
  see identical data.
