# Wikipedia Opensearch as a second test search backend

## Problem

Testing relies solely on DuckDuckGo (via the `ddgs` package), which gets rate-limited easily.
The existing `"wikipedia"` entry in `DDGS_ENGINES` (`oxe/search/engines/ddgs.py`) looks like an
alternative but is not one: it still routes through `ddgs`'s shared `HttpClient`
(`primp.Client(impersonate="random", ...)`), the same randomized-fingerprint transport DDG uses,
with no descriptive User-Agent. It is subject to the same anti-bot/rate-limit surface as
DuckDuckGo itself.

## Plan

1. **New module `oxe/search/engines/wikipedia.py`**: `WikipediaEngine` implementing the
   `SearchEngine` protocol directly, calling Wikipedia's Opensearch API
   (`action=opensearch&search=...&limit=...&namespace=0&format=json`) over stdlib
   `urllib.request` (mirrors `oxe/ai.py::_chat_completion`'s pattern, no new runtime dependency),
   wrapped in `asyncio.to_thread`. Sends a descriptive User-Agent
   (`oxe/{version} (...); test-backend`) per Wikipedia's API etiquette — this is the actual fix
   for rate-limiting, since the current DDG-routed path never sends one.
   - `name = "wikipedia-opensearch"` (distinct from the existing misleading `"wikipedia"` ddgs
     entry, so both can coexist).
   - `req.pageno` / `categories` / `time_range` / `safesearch` have no Opensearch equivalent;
     ignored, documented in the module docstring.
   - Narrow error handling (`HTTPError`, `URLError`, `TimeoutError`, `OSError`,
     `JSONDecodeError`) → `oxe.search.errors.BackendError`, per the repo's error-ladder
     convention.

2. **Registry wiring** (`oxe/search/engines/registry.py`): add
   `"wikipedia-opensearch": WikipediaEngine` to `_builtin_engines()`. Selection is then free via
   the existing `OXE_BACKENDS` env var — no new config surface: standalone
   (`OXE_BACKENDS=wikipedia-opensearch`), or fallback-composed
   (`OXE_BACKENDS='["wikipedia-opensearch","ddg"]'`).

3. **Tests** (`tests/search/test_wikipedia_engine.py`): mirror `test_ddgs_engine.py` — monkeypatch
   the HTTP call, no real network in unit tests. Cover result mapping, empty-results-is-not-an-
   error, HTTP/URL errors → `BackendError`, language-to-subdomain mapping, engine-field stamping.
   Add `"wikipedia-opensearch"` to `test_registry.py`'s builtin-discovery assertion plus a
   `test_resolve_named_wikipedia_engine` case. No cassette/VCR infra exists in this repo today;
   this plan does not introduce one.

4. **Optional**: an opt-in, skipped-by-default live-network smoke test
   (`OXE_TEST_LIVE_WIKIPEDIA=1`) for humans to verify the User-Agent/rate-limit story against the
   real API without adding flakiness to `mise run test:py`. Judgment call, not core scope.

5. **Docs**: one line in `oxe/search/__init__.py`'s module docstring (already lists `ddgs.py` /
   `searxng.py`) noting `wikipedia.py` as a third engines module and its purpose: a low-rate-
   limit, keyless dev/test backend for realistic-but-narrow data, not a production web-search
   substitute.

## Effort estimate

Roughly half a day: engine module + registry (~1-2h), tests (~1h), docs (~15min), optional live
smoke test (~30min). No new runtime dependencies, no schema/model changes.

## Critical files

- `oxe/search/engines/ddgs.py` (pattern to mirror)
- `oxe/search/engines/registry.py` (wiring point)
- `oxe/search/engines/protocol.py` (interface to implement)
- `oxe/ai.py` (urllib + `asyncio.to_thread` pattern to mirror)
- `tests/search/test_ddgs_engine.py` (test pattern to mirror)
