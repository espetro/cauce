# Reference insights: SearXNG

Analysis of the SearXNG "simple" theme as shipped on public instances (searx.be,
search.inetol.net, priv.au, paulgo.io). Source of truth is the repo itself
(`searx/templates/simple/`, `client/simple/src/`) since live instances are
bot-walled for automated fetches; markup below was read from the templates
directly.

## What it is

A server-rendered Flask metasearch engine that fans one query out to dozens of
upstream engines, merges and scores results, and serves plain HTML with a thin
vanilla TypeScript enhancement layer. No SPA, no framework, no client-side
rendering. The UI is the accumulation of a decade of restraint: every dynamic
behavior is an optional JS plugin over a page that must work without it
(there is a dedicated `sxng-noscript.min.css` bundle and an RSS output format
for the results page).

## Pages and surfaces

1. Landing (`index.html`): just a centered "SearXNG" title and the search box
   on a near-empty page. No news, no cards, no marketing.
2. SERP (`results.html`): three-region layout. Left: results column
   (`#urls`, articles). Right sidebar (`#sidebar`): infobox (Wikipedia-style
   Knowledge Graph answer, rendered as a `<details open>` collapsible),
   suggestions, "Messages from the search engines" (engine errors and
   response-time bar chart), search-url box for POST method, and a links box
   to the JSON/CSV/RSS API formats of the same query. Top: category tabs
   (general, images, videos, news, map, music, files, it, science) plus a
   preferences gear. Bottom: `#backToTop` affordance and either numbered
   pagination or the infinite scroll plugin.
3. Preferences (`preferences.html`, ~10k of template): a rare full
   preferences UI among search competitors. Structure is a radio-input
   tablist (no JS needed, `role=tab`/`aria-controls` present) with tabs for:
   General (interface language, autocomplete, safesearch, open results in new
   tab, theme), Engines (a big per-engine table with on/off toggles,
   reliability stats, response-time stacked bar charts, per-engine tooltip
   with description, website, wikidata link, and `!bang` shortcuts), Plugins
   (per-plugin toggles), Cookies, and an answer/LLM section on forks.
   Everything is a form POST with a save button at the bottom; JS only adds
   autosave-on-change.
4. Info/about: static `info.html`/`about.html` pages documenting how the
   engine works, privacy posture, and instance API usage.
5. Stats page: per-engine error logs and timings, linked from every failed
   engine name in the SERP sidebar.

Paging vs infinite scroll: numbered pagination is the default, URL-addressable
via `pageno`. Infinite scroll is an opt-in plugin (`plugin/InfiniteScroll.ts`,
enabled in preferences) that appends the next page's parsed HTML. The JS
bundle is code-split per plugin and dynamically imported only when active.

## State transitions

- Aggregation wait: none visible. The request blocks until the configured
  timeout, then renders the merged result set in one server response. There
  is no skeleton, no spinner, no progressive streaming of results. This is
  the single biggest UX difference from everything Perplexity-shaped.
- Partial failure: handled honestly and quietly. The sidebar's "Messages
  from the search engines" panel lists each unresponsive engine with its
  error type (timeout, captcha, blocked), each name linking to the stats
  page. When results exist the panel is collapsed and its summary shows
  "Response time: X seconds"; when nothing came back it is open. A partially
  failed aggregation still renders a full SERP; the user only loses engines,
  never the page.
- Empty state (`messages/no_results.html`): a `role=alert` dialog block that
  distinguishes page 1 ("Sorry! No results were found. You can try to:
  refresh, another query/category, change engines in preferences, switch
  instance via searx.space") from page N ("There are no more results").
  Concrete, actionable, per-page aware.
- Answers: SearXNG has an `answers` slot above results (used by calculator
  and similar instant-answer plugins), rendered before the sidebar.
- Corrections (did you mean) render above the result list.

## Motion

Almost none. `animations.less` exists but covers only tiny transitions
(hover states, the autocomplete dropdown, the image-detail modal). The
philosophy is explicit: server-rendered, works without JS, zero layout
shifting, no spinners. Even infinite scroll simply appends rows; the
image gallery opens a lightbox modal instead of navigating. Preferences
autosave is the only "invisible" enhancement. No skeleton screens anywhere,
because there is no async loading.

## AI integration (ecosystem, not core)

Core SearXNG has no LLM answer. The ecosystem bolts one on in four patterns:

1. Plugin injection into the answer box. `cra88y/ai-answers-searxng`: a
   single-file `post_search` plugin (OpenRouter/OpenAI/Ollama/Gemini/etc.)
   that injects a UI shell into the answer object; a client script streams
   tokens from a separate endpoint with HMAC-signed short-TTL tokens so
   result loading is never blocked. Features: token streaming, inline
   clickable citations, follow-ups, conversation state in the URL hash
   (`#ai=`), collapsed-by-default answer box to avoid layout shift, tab
   whitelist. This is the closest ecosystem analogue to oxe's AI mode.
2. Upstream proposal: searxng PR #4506 "Quick Answer" (OpenRouter, modeled
   on Mojeek summaries and Kagi Quick Answer), pending since 2025; stalled
   partly on a frontend-architecture conflict, showing even upstream
   considers the result column too narrow for long-form answers.
3. Forks that summarize results server-side (`Mikec78660/searxng-ai_summaries`)
   with per-result page fetching, configurable prompts, and a sidebar
   "request summary" toggle.
4. Wrappers consuming the JSON API: LangChain-SearXNG (RAG pipeline with
   LLM-selected search params and streaming SSE progress), searxng-ai-kit
   (CLI + MCP server exposing search to assistants).

Common lesson across all four: the aggregation layer (SearXNG, oxe) is the
RAG retrieval layer, and every AI integration streams answers separately
from the blocking result fetch.

## Layout and design system

- The "simple" theme is not Bootstrap; it is hand-rolled LESS with CSS
  custom properties, compiled by Vite into four bundles (ltr, rtl,
  noscript, rss). Older "oscar" lineage was Bootstrap 3; simple replaced it.
- Design tokens in `client/simple/src/less/definitions.less`: flat single-hue
  accents (`#3050ff` buttons/selected tab, `#000bbb` links, `#9822c3`
  visited), neutral `#444` body text on white, `#ddd` hairline borders.
  Error/warning/success tints. Everything is a `--color-*` variable, so
  theming is editing one file.
- Dark mode: automatic via `prefers-color-scheme` with a full parallel token
  set, plus a manual theme choice in preferences (auto/light/dark).
- Result anatomy (from `macros.html` `result_header`/`result_sub_footer`):
  `<article class="result result-default category-general">` containing
  favicon (14px, lazy), pretty-url breadcrumb split into spans
  (`.url_wrapper` with per-part spans, Google style, above the title), `<h3>`
  title link, optional proxied thumbnail with duration overlay, published
  date/author/metadata in sub-header, and the sub-footer: a `.engines` row of
  plain `<span>` chips naming every engine that returned this result, an
  ellipsis icon, and a `cached` link through a cache proxy. No score is shown
  to users (scoring is internal; the per-engine ranking contribution is in
  admin stats).
- Result templates per category (default, images, videos, torrent/files,
  map, code, key-value/paper for science); `results.html` groups same-template
  runs into grid containers for image/file walls.
- Type scale is modest: h1 title on landing, h3 result titles, small
  metadata; density is high, decoration near zero.

## Comparison: engines chips vs oxe cache meta line

SearXNG's per-result `.engines` chip row is unique transparency: it tells you
which of many upstream sources produced each hit, with a cached-link and a
link to error logs. It answers "who said this" per result. oxe's meta line
(search.md) answers a different question set: "is this from cache, how old,
and how do I refresh or share it" via the clickable `cached · 3h old` badge
plus `copy link`/`copy json`. SearXNG shows provenance but hides freshness
and machine access (JSON/RSS links sit in a sidebar box, not per result); oxe
shows freshness and machine addressability but, with a single upstream (DDG),
has no per-result provenance story to tell. The interesting borrow is not the
chips themselves but the failure surface: SearXNG surfaces per-source errors
and timings even when the page succeeds.

## Applies to

- Screen: Search results (`.agents/docs/screens/search.md`): meta line
  ordering, cache badge, empty state, continuous scroll vs page fallback.
- Screen: AI mode (`&mode=ai`): streaming-answer patterns validated by the
  SearXNG plugin ecosystem (non-blocking fetch, collapsed answer box, URL
  state, citations).
- Checkpoints: cache/source transparency meta line, engine/source failure
  messaging, empty state on page 1 vs page N, no-JS degradation.

## Sources

No local screenshots exist for SearXNG; this analysis is from repo sources
and project docs. Live instance fetches (searx.be, search.inetol.net) were
attempted but blocked by antibot challenge pages.

- https://docs.searxng.org/ (admin/user docs)
- https://github.com/searxng/searxng (searx/templates/simple, client/simple/src)
- https://raw.githubusercontent.com/searxng/searxng/master/searx/templates/simple/results.html
- https://raw.githubusercontent.com/searxng/searxng/master/searx/templates/simple/macros.html
- https://raw.githubusercontent.com/searxng/searxng/master/searx/templates/simple/messages/no_results.html
- https://raw.githubusercontent.com/searxng/searxng/master/searx/templates/simple/elements/engines_msg.html
- https://raw.githubusercontent.com/searxng/searxng/master/searx/templates/simple/preferences.html
- https://raw.githubusercontent.com/searxng/searxng/master/client/simple/src/less/definitions.less
- https://raw.githubusercontent.com/searxng/searxng/master/searx/static/themes/simple/manifest.json
- https://searx.space/ (instance list context)
- https://github.com/cra88y/ai-answers-searxng
- https://github.com/searxng/searxng/pull/4506
- https://github.com/searxng/searxng/discussions/5661
- https://github.com/Mikec78660/searxng-ai_summaries
- https://github.com/nikvdp/searxng-ai-kit
- https://github.com/ptonlix/LangChain-SearXNG

## Verdict for oxe

1. Do not copy the blocking aggregation wait: SearXNG renders nothing until
   timeout. oxe's cache-first architecture means the cached SERP should
   render instantly and re-fetch only on the explicit `cached` badge click,
   which is a UX advantage to keep.
2. Steal the failure surface: a collapsed "source messages" area that names
   failed sources with error type and stays silent when everything worked
   (searx/templates/simple/elements/engines_msg.html). For oxe this maps to
   DDG failures and cache staleness, not engine chips.
3. Steal the empty state: distinct page-1 ("no results, try X") vs page-N
   ("no more results, go back") copy with concrete actions, `role=alert`.
4. AI mode design validation: every successful SearXNG AI integration keeps
   the answer fetch non-blocking, collapses the answer box to prevent layout
   shift, streams tokens with citations, and keeps conversation state in the
   URL. oxe's AI mode already matches this; treat the collapsed-box-on-load
   and citation-click behaviors as required, not nice.
5. The preferences panel is SearXNG's moat among simple engines because it
   exposes engine reliability data (per-engine toggles, error rates,
   timings). oxe's equivalent transparency budget should go into cache
   freshness/refresh affordances rather than an engine table, since there is
   one upstream.
