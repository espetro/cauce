
## 2026-09-15 Stage 3 AI pipeline (feat/v0.4.0-web-ui)
- Implemented: oxe/config.py (TOML loader, OXE_CONFIG_DIR env, AIConfig, ConfigError; AI off unless provider+model), oxe/ai.py (manual ReAct loop over aisuite, max 5 iters, confidence>=8 stop, SSE events step/delta/sources/done), oxe/sql/answers.sql + schema answers table, TTLCache.get_answer/put_answer/answer_stats, POST /answer (SSE, cache hit = single done with cached:true, 409 when unconfigured), GET /v1/models (direct openai/anthropic SDK, [] + ai_available:false when off), exa_search gained source=web|history|cache (tool_source key in response).
- pyproject: ai = ["aisuite[openai,anthropic]~=0.1.14"] extra; all provider imports lazy (verified: importing oxe.ai loads no aisuite/openai/anthropic).
- Tests: tests/test_ai_pipeline.py 20 tests, mocked aisuite client via monkeypatch of oxe.ai._aisuite_client; no network; suite 55 passing (35 baseline + 20).
- Gotcha: aisuite streaming chunks are openai-shaped; tool_calls must be accumulated manually from deltas (index-merged). Loop consumes stream in a worker thread; event loop polls the done Event.

## 2026-09-15 v0.4.0 shipped (feat/v0.4.0-web-ui, tag v0.4.0)
- Full stack: Preact+Vite+daisyUI UI (pill search bar, Search/AI toggle, letters pager, settings dialog w/ theme, model combobox, ?settings=open URL state), AI pipeline (POST /answer SSE, /v1/models, oxe[ai] aisuite~=0.1.14, answer cache), /suggest (search_log) + /ac (DDG proxy), pagination (page in cache key), config.toml with {env.VAR} interpolation + .env loading, GET/PUT /settings, dev observability (OXE_DEV structured logs, mise run logs).
- Legacy oxe/static templates + ui.py deleted; SPA served from OXE_UI_DIST (mise build:ui copies ui/dist to oxe/ui_dist for packaging).
- Gates at release: 84 pytest, 32 bun tests, ruff clean (config added to pyproject), JS 21.4KB/CSS 16.0KB gz budgets, RSS ~22MB (56MB with ai extra) vs 70MB baseline.
- URL contract for QA agents: q, p (absent=1), mode=ai, settings=open|close; documented in ui/AGENTS.md + .agents/docs/screens/userflow-checkpoints.md.
- Process: validator agents MUST stay read-only (two violations logged; see memory validator-generator-separation.md). Layout quality needed 2 iterations (cramped landing -> 672px pill @43.8% vh, scored 8.5/10).
- AI testing: .env has OXE_AI_* keys; config supports api_key = "{env.OXE_AI_API_KEY}".

## 2026-09-15 - v0.4.0 P0 fixes (feat/v0.4.0-web-ui, uncommitted)

- **aisuite 0.1.14 Client drops the `tools` kwarg** unless `max_turns` is set: `Completions.create` pops `tools` at client.py:330 and only re-injects it in the `_tool_runner` path. This silently disabled the whole ReAct loop. Fix: `oxe/ai.py::_sdk_client` now uses the raw openai/anthropic SDKs directly (aisuite kept only as legacy `_aisuite_client` for tests).
- OpenRouter free models emit `<tool_call>...<arg_key>` text instead of the tool_calls protocol on some prompts; `TEXT_TOOL_CALL_RE` in oxe/ai.py catches this and surfaces "model does not support tool calling" instead of streaming raw JSON. `_friendly_provider_error` maps 401/404/429/402 to short messages.
- DDG html paging: ddgs `page` kwarg works but is aggressively rate-limited ("No results found." on burst). exa_compat retries each backend once after 1.5s; oxe/search.do_search caches page>1 payloads with short `_PAGE_TTL=300`.
- POST /settings/test + UI "Test connection" button (ui/src/features/settings/SettingsDialog.tsx, lib/ai.ts::testConnection). Settings prefill now hydrates provider/model/base_url/enabled from GET /settings on open (controlled provider select + ModelPicker value; namedItem for uncontrolled fields).
- Theme contrast: page bg = base-200 (light) / darkened base-300 via oklch rel color (dark) via `--oxe-page` in ui/src/index.css; pill/cards stay base-100. Lightning CSS downlevels oklch-from for old browsers.
