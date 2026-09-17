
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

## 2026-09-16 AI-mode six-issue audit (ui/)
- Only real code defect: duplicate `rerunOnQueryChange` effect in ui/src/routes/search.tsx (removed; one copy remained). Rest of interrupted diff was complete: stripAnswerMeta (useAnswer.ts), busy→AiControls reasoning disable, ModelPicker/oxe-pill-control, useSearchMode single-source-of-truth, AnswerView dots-only streaming indicator, en.json copy.
- Stale-bundle trap: server serves ui/dist from disk; after src edits run `bun run build` in ui/ AND hard-reload. Chunk-name map: search-*.js = /search route + useAnswer + AnswerView; routes-*.js = Home; SearchBox-*.js = useSearchMode/AiControls. Verify with `curl -s :4479/assets/<chunk> | grep <symbol>`.
- Backend behavior (not a UI bug): SSE `done` for fresh web-search queries can take minutes (ReAct loop, max 5 iters); during the wait reasoning toggle stays disabled (correct). Cached answers return a single done event instantly.

## 2026-09-17 v0.5.0 plan (research complete, awaiting owner answers)

9 research notes in `.agents/drafts/research/2026-09-17-*.md`: architecture-audit, ai-chat-ui-libs, headless-ui-libs, chat-ui-patterns, framework-migration-scoring (take 6), fastapi-frontend-api, fsm-options, local-app-js-budget, dependency-coupling. Plan at `.agents/plans/2026-09-17-v0.5.0-plan.md` (6 streams, 17 atomic commits).

**Size budget re-derived** (`ui/scripts/size-budget.ts`): JS_TARGET 100 KB gz, JS_BUDGET 150 KB gz, RECALIBRATE 130 KB gz (WARN), CSS_BUDGET 60 KB gz. Old 45 KB cap was a mobile-3G heuristic; localhost SPA on 2026 desktop has vastly more headroom. Current built SPA = ~37 KB gz JS, ~22 KB gz CSS.

**Framework verdict (take 6, final)**: **stay on Preact + Vite (3/10 to migrate)**. Scored against 5 frameworks out of ~80 surveyed. Lifting Preact-loyalty constraint + widening budget does NOT flip the math. React migration is mechanical (~2843 LOC across 21 files, ~3-5 dev-weeks); question is whether React's chat-lib ecosystem justifies it -- today no.

**FastAPI app.frontend() swap** (6/10 win, framework-independent): ship in FastAPI 0.138.0 (2026-06-20, PR #15800). ~60-75 LOC removable from `oxe/server/system.py` and `cache_admin.py`. Two commits: pin bump + frontend swap.

**FSM pick for v0.5.0**: **useReducer + discriminated unions (8/10)**. Zero dep, sufficient for 3-5 state widgets. xstate v5 = 6/10 (~15-17 KB gz). `@xstate/fsm` deprecated in v5. ReScript = 1/10.

**Headless UI pick**: **Zag.js** for popover/tooltip/combobox/dialog (+4-6 KB gz). Radix/Ark rejected (Radix unsupported on Preact via official channel). Keep nanostores + virtua.

**AI chat UI**: roll-your-own minimal state machine + transport adapter (38/40) beats Vercel AI SDK (20), assistant-ui, NLUX, TanStack AI (29). Current SSE format: `step | delta | sources | done`.

**Architecture bugs confirmed (Stream 2)**:
- `delete_clicks` only deletes `clicks` table, not `search_log` -> dashboard stays stale after "delete all"
- `7d` and `30d` scopes return 0 silently -- bug in `oxe/cache.py:331-340`
- History rows link to `/search?q=...` without mode -> mode-preservation lost on click
- `search_log` has `source` column but no `mode` -- needs schema addition
- Dashboard reads `search_log` (via `/api/stats`), History reads `clicks` -- unsynced data planes

**Repo size**: ui/ = 17,065 LOC / 384 files excl node_modules+dist. Of 167 .js files in ui/src/, 161 are generated `ui/src/paraglide/messages/` i18n files. Real hand-written JS = ~1 file.

**10 owner questions blocking execution** (see plan section 4):
1. Delete scope semantics
2. x-cache* headers on `/search` HTML branch
3. JSON 404 envelope shape
4. AI-mode logging (extend search_log.source vs new answer_log table)
5. `/cache` page scope (admin-only or per-row delete for all)
6. Tools toggle default for first-time AI users
7. Auto-title LLM call (separate cheap call vs derive from first user message)
8. Tab persistence (localStorage vs sessionStorage)
9. Multi-turn caching (last-N turns vs final-answer-only)
10. Branch/regenerate scope (v0.5.0 or v0.6)

**Workflow conventions**:
- `.agents/research/` is DEPRECATED -- use `.agents/drafts/research/` (gitignored), `.agents/plans/`, `.agents/docs/` only
- For narrow delegation, use `model_tier: weak` to avoid timeouts
- No em-dashes in docs (rule 7 of global AGENTS.md)
