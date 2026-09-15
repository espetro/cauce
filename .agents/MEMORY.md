
## 2026-09-15 Stage 3 AI pipeline (feat/v0.4.0-web-ui)
- Implemented: oxe/config.py (TOML loader, OXE_CONFIG_DIR env, AIConfig, ConfigError; AI off unless provider+model), oxe/ai.py (manual ReAct loop over aisuite, max 5 iters, confidence>=8 stop, SSE events step/delta/sources/done), oxe/sql/answers.sql + schema answers table, TTLCache.get_answer/put_answer/answer_stats, POST /answer (SSE, cache hit = single done with cached:true, 409 when unconfigured), GET /v1/models (direct openai/anthropic SDK, [] + ai_available:false when off), exa_search gained source=web|history|cache (tool_source key in response).
- pyproject: ai = ["aisuite[openai,anthropic]~=0.1.14"] extra; all provider imports lazy (verified: importing oxe.ai loads no aisuite/openai/anthropic).
- Tests: tests/test_ai_pipeline.py 20 tests, mocked aisuite client via monkeypatch of oxe.ai._aisuite_client; no network; suite 55 passing (35 baseline + 20).
- Gotcha: aisuite streaming chunks are openai-shaped; tool_calls must be accumulated manually from deltas (index-merged). Loop consumes stream in a worker thread; event loop polls the done Event.
