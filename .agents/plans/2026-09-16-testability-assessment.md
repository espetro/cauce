# oxe testability assessment

Date: 2026-09-16. Read-only research. Inputs: four parallel subagent POVs (DI seam audit, mocking/stubbing audit, frontend testability, component-by-component scorecard).

## TL;DR

oxe is mostly well-designed for testing (DI at every major boundary, dataclasses for state, frozen SQL via `aiosql`), but two structural problems dominate the debt:

1. **Module-level `cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))` at `oxe/server/state.py:41`** runs the moment anyone imports `oxe.server` and opens the real `~/.cache/oxe/cache.db` on the developer's machine. No `tests/conftest.py` exists to redirect `OXE_CACHE_DIR` before import, and `grep -r OXE_CACHE_DIR tests/` returns zero hits — every test currently leaks a file descriptor to the real cache dir just to import `make_app`.
2. **MCP server has zero direct test coverage.** `oxe/mcp_server.py:19` constructs `MCPServer(...)` at import; tools are module-level decorators using a module-level `_cache` global; there is no `test_mcp*.py`. Tools are only reachable through the FastAPI mount in `make_app`.

Everything else is polish on top of these. A 60-line `tests/conftest.py` + a 100-line `tests/test_mcp_server.py` would close ~70% of the gap with no production code changes.

---

## 1. Current test surface (ground truth)

### Python: 12 test files, ~50 KB of tests

- `tests/test_ai_pipeline.py` (24 tests) — config loader, answer parser, loop behaviour, tool exec, source dedup, greeting guard, cached round-trip. Uses `_fake_client` + `monkeypatch.setattr(ai_mod, "_sdk_client", …)`. Strongest file.
- `tests/test_cache_sql.py` (13 tests) — every test creates `TTLCache(tmp_path / "c.db")` + `c.close()`. Cleanest file.
- `tests/test_backends.py` (12 tests) — `Fake`/`Slow` classes + `protocol` duck-typing. Gold-standard DI.
- `tests/test_settings.py` (20 tests) — TOML roundtrip, `{env.*}` interpolation survival, base_url keep, PUT semantics.
- `tests/test_pagination.py` (8 tests) — page semantics, retry. `patch.object(exa_compat, "DDGS", FakeDDGS)` everywhere.
- `tests/test_error_annotation.py` (4 tests) — X-Cache header, failure annotation. Same DDGS pattern.
- `tests/test_api_history.py` (12 tests) — merge, filters, validation, delete, redirect. `_seed()` fixture seeds the cache.
- `tests/test_api_stats.py` (6 tests) — JSON contract.
- `tests/test_suggest.py` (8 tests).
- `tests/test_openapi_fresh.py` (2 tests) — **uses module-level `app` → leaks `~/.cache/oxe/cache.db`** (see issue #1).
- `tests/test_ui_dist.py` (6 tests).
- `tests/test_templates.py` (1 test).
- `tests/test_devlog.py` (4 tests).

### UI (Preact + Vite): 6 test files, ~299 LOC of tests, ~14% source coverage

- Runner: `bun test src` (`ui/package.json:13`). No vitest, no @testing-library, no jsdom/happy-dom.
- `lib/format.test.ts` (8 tests) — pure helpers.
- `components/ErrorBoundary.test.ts` (3 tests) — vnode assertions only.
- `features/answer/useAnswer.test.ts` (7 tests) — pure `applyAnswerEvent` reducer.
- `features/search/useSearch.test.ts` (5 tests) — pure helpers.
- `features/search/pager.test.ts` (4 tests) — URL builder.
- `features/suggests/useSuggests.test.ts` (4 tests) — `mergeSuggests` pure logic.

**No** test for any `.tsx` component. **No** test for `lib/api.ts`, `lib/ai.ts` SSE loop, `lib/theme.ts` localStorage, any route (`routes/*.tsx`), or any feature component (`features/*/*.tsx`). The bun runner has no DOM; no `@testing-library/preact` is installed.

### Python DI pattern (very good)

Almost every Python module exposes a constructor or free function that takes its dependencies:

```python
TTLCache(db_path)                              # oxe/cache.py:23
do_search(cache, req, backend=None, on_result=None, with_duration=False)   # oxe/search.py:18
make_app(cache=None, backend=None, on_result=None)                         # oxe/server/app.py:25
AiConfig(provider, model, api_key=…)           # oxe/config.py:34
FallbackBackend([b1, b2]); FanoutBackend([b1, b2])                        # oxe/backends.py:127, 161
run_one(backend, req, timeout=None)             # oxe/backends.py:104
load_config(path=None)                          # oxe/config.py:148
build_from_env() / resolve(spec)                # oxe/registry.py:84, 60
stream_answer(query, cfg, cache=None, backend=None, on_result=None, max_iterations=5)  # oxe/ai.py:244
build(db_path, out_dir, days=30) / build_json(db_path, days=14)            # oxe/stats.py:348, 297
```

`SearchBackend` and `CacheAdapter` are `@runtime_checkable` Protocols (`oxe/backends.py:40, 50`). Tests duck-type via `class Fake: name=...; timeout=...; def search(self, req): …`.

---

## 2. Component-by-component seam scorecard

Score key: **5** = pure DI, no import side effects. **1** = module singleton, untestable without global mutation.

| # | Component | Seam | Test files | Construction pattern |
|---|---|---:|---|---|
| 1 | `TTLCache` | **5** | `test_cache_sql.py` (13) | ctor + `db_path` |
| 2 | `do_search` | **4** | `test_pagination.py`, `test_error_annotation.py` | free fn; backend injectable; TTL frozen at import |
| 3 | `exa_compat.search/cache_key/_dgr_to_exa/build_query` | **3** | `test_pagination.py`, `test_error_annotation.py` | free fns; `DDGS` is a from-import that must be monkeypatched |
| 4 | `DDGBackend`/`DdgsBackend`/`FallbackBackend`/`FanoutBackend`/`run_one` | **5** | `test_backends.py` (12) | pure ctor + Protocol duck-type |
| 5 | `registry.build_from_env`/`resolve`/`discover` | **4** | `test_backends.py:110-160` | env-driven + pure fns |
| 6 | `config.load_config`/`save_config`/`AIConfig` | **5** | `test_ai_pipeline.py:24-69`, `test_settings.py:92-147` | dataclass + `path=` ctor |
| 7 | `ai.stream_answer`/`_sdk_client`/`build_toolset`/`parse_final_answer`/`_sources_from_messages`/`_greeting_error`/`answer_cache_key`/`test_provider_connection` | **4** | `test_ai_pipeline.py` (24), `test_settings.py:289-311` | free fns; `_sdk_client` + `build_toolset` monkeypatched |
| 8 | `server.make_app` + handlers (search/ai/cache_admin/system) | **5** / **4** | 8 test files | `make_app(cache, backend, on_result)`; per-router `build_router(state)`; env-driven TTLs frozen at import |
| 9 | `mcp_server.mcp` + tools (`exa_search`, `exa_user_history`) | **2** | **none** | module-level `MCPServer` + module-level `_cache` set via `set_cache()` |
| 10 | `devlog.event`/`dev_enabled`/`DEV` | **4** | `test_devlog.py` (4) | free fns gated by env; `DEV` cached at import |
| 11 | `stats.build`/`build_json` + `panel_*` | **5** | only HTTP via `test_api_stats.py` | pure fns taking `db_path`; HTML + panels untested |
| 12 | `sqlload.queries`/`schema_sql` | **4** | transitively via cache tests | lazy `_queries` module cache; package-data path |
| 13 | `__main__.main` (CLI) | **3** | none | argparse + `uvicorn.run`; `from .server import app` opens live cache even for `oxe stats` |
| 14 | UI: routes, components, hooks | **2-4** | 6 files (~299 LOC) | preact-iso routing; pure components + reducers |

---

## 3. Module-level side effects at import

| Import | Side effect | Source | Testability impact |
|---|---|---|---|
| `import oxe.server` | `app = make_app()` → `TTLCache(CACHE_DIR/cache.db)` opens real SQLite; `build_from_env()` reads `OXE_BACKENDS`; `set_cache(c)`; MCP sub-app mounts | `oxe/server/__init__.py:14` + `oxe/server/state.py:41` | **High** — every test file does `from oxe.server import make_app` and triggers this |
| `import oxe.server.config` | `OXE_PORT`/`OXE_CACHE_DIR`/`OXE_LOG_LEVEL` reads + `logging.basicConfig(...)` | `oxe/server/config.py:6-13` | High — `basicConfig` is process-global and sticky |
| `import oxe.server.search` | loads `exa_compat` → `from ddgs import DDGS` (real package load) | `oxe/exa_compat.py:9` | Low — patched in tests |
| `import oxe.devlog` | `DEV = dev_enabled()` snapshots env | `oxe/devlog.py:20` | Low — `event()` re-checks per call |
| `import oxe.search` | freezes `TTL_DEFAULT`/`TTL_MAX`/`NEGATIVE_TTL` | `oxe/search.py:12-14` | Medium — tests can't `monkeypatch.setenv` to override mid-run |
| `import oxe.cache` | none | — | — |
| `import oxe.sqlload` | none (lazy) | — | — |
| `import oxe.ai` | none (SDK imports lazy) | — | — |

---

## 4. Mocking / pre-seeding patterns in use

### Patterns that work

- **DDG mock**: `patch.object(exa_compat, "DDGS", FakeDDGS)` used 6+ places (`test_pagination.py:42,57,74,98,123`, `test_error_annotation.py:27,50,63,74,94`).
- **AI provider mock**: `_fake_client(responses)` returning scripted turns via `monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, …))`.
- **SDK mock**: `patch.dict("sys.modules", {"openai": fake_mod})` with `MagicMock` for `/v1/models` (`test_settings.py:195,200,219,239`).
- **Backend mock**: `class Fake: name=...; timeout=...; def search(self, req): …` — the protocol is duck-typed.
- **Cache seed**: every test creates `TTLCache(tmp_path / "c.db")` and calls `.close()`. SQLite isolation is hermetic.
- **Time control**: `ttl=-1` for "already expired" — avoids `time.time()` brittleness.
- **Config override**: `monkeypatch.setattr(cfg_mod, "config_path", lambda: tmp_path / "absent.toml")` redirects `load_config` to tmp.

### Patterns missing

- **No `fetch` abstraction** in `ui/src/lib/api.ts` — tests would need `mock.module(globalThis.fetch)` or MSW (untested with bun).
- **No `LocationProvider` test wrapper** — `preact-iso` `useLocation()` has no mock.
- **No DOM in bun:test** — component tests are impossible without happy-dom preload.
- **No entry-point plugin test** — `registry.discover()`'s `entry_points(group="oxe.backends")` branch is unexercised.
- **No `OXE_CACHE_DIR` session-scope override** — the single biggest gap.

---

## 5. Testability debt — ranked

| # | Issue | Severity | Effort | Fix |
|---|---|---|---|---|
| 1 | **`OXE_CACHE_DIR` not isolated in tests.** Module-level `cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))` at `oxe/server/state.py:41` opens real `~/.cache/oxe/cache.db` the moment `oxe.server` is imported. No test sets `OXE_CACHE_DIR`. Even with `make_app(cache=...)`, the default singleton still runs. | **High** | **S** (10-15 lines) | Add `tests/conftest.py` with `autouse=True` fixture that sets `OXE_CACHE_DIR = tmp_path_factory.mktemp("oxe-cache")` and `monkeypatch.delenv("OXE_CONFIG_DIR")` BEFORE any `from oxe.server import make_app`. Single line of `pytest.ini_options.addopts = "--import-mode=importlib"` + a session-scoped autouse env-redirect. |
| 2 | **MCP server has zero tests.** `oxe/mcp_server.py:19` constructs `MCPServer` at import; tools read module-global `_cache` set via `set_cache()`. The only way to exercise tools today is via the FastAPI mount, which means a full `mcp.streamable_http_app()` handshake. | **High** | **M** (1-2h) | Refactor `exa_search` / `exa_user_history` into pure functions `_exa_search_impl(cache, **kwargs) -> dict`; keep the `@mcp.tool` decorators as thin wrappers calling them. Add `tests/test_mcp_server.py` with `TTLCache(tmp_path / "db")` + `set_cache(c)` + direct calls to the impls. |
| 3 | **`logging.basicConfig(...)` at `oxe/server/config.py:10`** is sticky process-global state. Test ordering or a future library import could silently lose log level. | **High** | **S** | Guard with `if not logging.root.handlers: logging.basicConfig(...)`. Defer until first log call. |
| 4 | **`OXE_TTL_DEFAULT`/`MAX`/`NEGATIVE_TTL`/`_PAGE_TTL` frozen at import** (`oxe/search.py:12-16`). Tests can't `monkeypatch.setenv` to override; the constants are already snapped. | Medium | **S** | Convert to module-level helper functions `_ttl_default()`, etc. Cheap. |
| 5 | **`/ac` route (`oxe/server/search.py:131-151`) hits live DDG.** No tests cover the success path because it requires internet. | Medium | **M** | Add `ac_client: Callable | None` kwarg to `make_app`, default to a real-network adapter; tests inject a fake list. Or `monkeypatch.setattr(oxe.server.search, "fetch_ddg_ac", lambda q: [])`. |
| 6 | **`stats.py` panel renderers (`panel_*`) and HTML output untested.** `test_api_stats.py` only covers the JSON sister. A regression in `panel_latency` percentile math ships silent. | Medium | **S** | Add `tests/test_stats.py` — seed `search_log`, assert every `panel_*` returns `<svg ...>` or `<p class="empty">no data yet</p>` for empty input. |
| 7 | **`devlog.DEV` captured at import** (`oxe/devlog.py:20`). Tests can flip `OXE_DEV=1` mid-run but the constant stays false until re-import. | Low | **S** | Delete `DEV = dev_enabled()` (line 20); `event()` already re-checks per call. |
| 8 | **`__main__.main` opens live cache even for `oxe stats`.** Hard import `from .server import app` (`__main__.py:6`). `oxe stats --db /tmp/x.db` still creates `~/.cache/oxe/cache.db`. | Medium | **S** | Move `from .server import app` into the `if args.command != "stats"` branch (`__main__.py:32`). |
| 9 | **UI has no DOM in tests.** `bun test src` has no `@testing-library/preact`, no `happy-dom`. Zero component tests possible. | High | **M** (1-2h) | Add `happy-dom` dep + `bunfig.toml` `test.preload = ["./test-setup.ts"]` that does `import {Window} from "happy-dom"; Object.assign(globalThis, new Window())`. Opens the door to every component test. |
| 10 | **`MarkdownLite` regex rendering + `ModelPicker` keyboard nav untested.** Both leaf-component, high-value surface. | Medium | **M** | Once happy-dom is in, render-and-fire-event tests for both. Catches AI answer regressions. |
| 11 | **`routes/search.tsx` (348 LOC) and `routes/history.tsx` untested.** Heaviest UI files. URL-state contract means most behavior is testable as pure render under `<LocationProvider url="..."/>`. | Medium | **L** | Add `preact-iso` test wrapper + tests per route covering the URL params (`?q`, `?p`, `?mode=ai`, `?settings=open`, `?since=`, `?qf=`). |
| 12 | **`?force=error|ai-off|empty` + `?suggest=1` URL stubs not implemented.** QA matrix has 5 un-pin-able checkpoints. | Medium | **S** (UI) | Stub `lib/api.ts` to read `URLSearchParams` and short-circuit; closes the QA gap. |

---

## 6. What's already clean (notable)

- **TTLCache** — full DI, tests close explicitly, gzip+JSON+SQLite works hermetically.
- **backends.SearchBackend / CacheAdapter Protocol** — `@runtime_checkable`, duck-typed throughout. The `Fake` class in `test_backends.py:15-30` is the textbook example.
- **do_search** — pure function; observer param is the seam; `_notify` is defensive try/except.
- **AIConfig** — dataclass; serialization-aware (`to_toml` keeps env templates round-trip-safe).
- **`make_app(cache, backend, on_result)`** — every dependency injectable.
- **`AppState` dataclass + per-router `build_router(state)`** — single seam in the entire FastAPI app.
- **aiosql named queries** — SQL lives in `.sql` files; tests can't drift from production SQL.
- **DDG mock pattern** — `patch.object(exa_compat, "DDGS", FakeDDGS)` is consistent across 6+ tests.
- **`_fake_client` in `test_ai_pipeline.py:99`** — scripted-turn builder for AI tests is elegant.

---

## 7. Missing tests ranked (with effort)

| Gap | Effort | Why it matters |
|---|---|---|
| `tests/conftest.py` redirecting `OXE_CACHE_DIR` and `OXE_CONFIG_DIR` to `tmp_path` session-scope | **S** (10-15 lines) | Closes #1 debt. Every existing test gets hermetic FS for free. |
| `tests/test_mcp_server.py` covering `exa_search` (cache/history/web branches) + `exa_user_history` (limit/since_hours clamping, cache-unset fallback) | **M** | Zero MCP coverage today; refactor tools to pure functions first. |
| `tests/test_stats.py` covering `panel_*` renderers + percentile math | **S** | HTML output regression shield for the static dashboard. |
| `tests/test_sqlload.py` asserting every `oxe/sql/*.sql` file loads via `queries()` | **S** | Catches "forgot to add new sql to package-data". |
| `tests/test_cli_stats.py` invoking `_run_stats` with tmp_path db | **S** | After lazy-import fix (#8), CLI gets coverage. |
| `tests/test_devlog_integration.py` asserting `event("search", …)` fires through `oxe.server.search` when `_DEV=True` | **S** | Call sites unverified today. |
| `tests/test_httpx_search.py` mocking `_ac_client` / `urlopen` for `/ac` happy + error paths | **M** | Closes #5. |
| Frontend: `ui/test-setup.ts` with happy-dom preload + `ui/bunfig.toml` `test.preload` | **M** (1-2h) | Unblocks all frontend component tests. |
| `ui/src/lib/api.test.ts` using `mock.module(globalThis.fetch)` for fetch wrappers | **S** | Cheap fill for biggest frontend coverage hole. |
| `ui/src/features/settings/schema.test.ts` validating the valibot schema | **S** | Pure function, high value. |
| `ui/src/features/answer/MarkdownLite.test.ts` covering citation regex, code blocks, headings | **M** | AI answer surface, high regression risk. |
| `ui/src/components/ModelPicker.test.ts` keyboard nav (ArrowDown/Up/Enter/Escape) + outside-click | **M** | A11y-relevant, complex logic. |
| `tests/e2e/` Playwright suite covering 12 "works today" checkpoints from `userflow-checkpoints.md` | **L** (half-day+) | Highest-ROI UI gate once `?force=` stubs + happy-dom are in. |

---

## 8. Suggested sequencing for v0.5

1. **PR 1 (1-2h, no behavior change)**: `tests/conftest.py` + `tests/test_stats.py` + `tests/test_sqlload.py` + `tests/test_cli_stats.py`. Locks the floor.
2. **PR 2 (1-2h)**: refactor MCP tools to pure functions + `tests/test_mcp_server.py`. Closes #2 debt.
3. **PR 3 (1h)**: `devlog.DEV` removal + lazy `__main__` import + `logging.basicConfig` guard + TTL helper functions. Pure cleanup.
4. **PR 4 (UI, 1-2h)**: happy-dom preload + `ui/src/lib/api.test.ts` + `ui/src/features/settings/schema.test.ts`. Lifts frontend coverage from ~14% to ~30%.
5. **PR 5 (UI, half-day)**: MarkdownLite + ModelPicker + (if room) routes/search.tsx tests. The big UX surface gets a regression net.
6. **Future**: Playwright e2e for the 12 working checkpoints. Once happy-dom is in, this is a much smaller lift.

Net effect: test coverage goes from "all Python handlers + cache + AI pipeline, no MCP, no stats panel, no UI components" to "all of the above + MCP + stats + UI reducers + happy-dom component tests + a Playwright regression net for the URL-state contract."

## 9. Recommendations for owner sign-off

Three questions before prioritizing PRs:

- **Q1**: Should `OXE_CACHE_DIR` isolation land as a session-scope autouse in `tests/conftest.py` (low-friction, all tests hermetic) OR as a refactor of `state.py:41` to lazy-construct the cache (cleaner but a code change)? Default recommendation: conftest first, refactor later.
- **Q2**: Is MCP tool surface considered stable enough that breaking changes during the pure-function refactor are fine? Default recommendation: yes — the MCP wire format (function name + schema) is what matters; renaming internals is fine.
- **Q3**: Is `happy-dom` an acceptable UI dev-dep? It'd add ~2 MB to `bun install` but unlocks all component tests. Default recommendation: yes.

If owner says "no new deps": scope shrinks to PRs 1-3 (Python only). If owner says "go big": add PR 4-5 and the Playwright e2e.
