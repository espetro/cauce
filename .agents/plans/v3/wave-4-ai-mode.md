# W4: AI mode

Iteration 5 (2026-11-17 to 2026-11-30). Priority P2. EPIC issue: #5.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-3-tail-tolerance.md`.

## Goal

A streamed, grounded answer surface over the search pipeline, via any OpenAI-compatible
endpoint (Bifrost by default), with answers cached only when they cite sources, and evals
that catch regressions offline. Anthropic Messages protocol as the last step.

## Settled inputs

- Provider protocol: OpenAI chat completions with `stream: true` and tool calling; config
  `[ai] enabled, base_url, api_key (template), model, protocol = "openai" | "anthropic"`
  (anthropic implemented in W4-05). Defaults from W0-11.
- Answer frames over SSE, typed: `step{tool, query, label}`, `delta{text}`,
  `sources{sources: [{url,title,snippet,engine}]}`, `done{answer, confidence, model,
  related_questions, cached, request_id}`, `error{message, retry_after_s?}`. Exposed as
  `POST /api/answer` (routes table) and HTMX page `/answer?q=`.
- Caching rule: `answers` row written only if `sources.len() >= 1` and `confidence >= 4`
  and no error; TTL 24 h; key = (normalised q, model).
- Tool loop cap 5 iterations; tools exposed to the model: `search_web` (pipeline, so it
  benefits from cache/admission/politeness), `search_archive` (W5, absent until then).
- Evals: `evals/ai/*.jsonl` with `{"query", "must_cite_domains", "must_contain",
  "must_not_contain"}` run over replay cassettes with a recorded provider transcript
  (`evals/ai/transcripts/`); baseline score in `evals/thresholds.toml`; CI fast gate runs 5
  cases (< 30 s), full set offline via `oxe eval ai`.
- Resource-adaptive: no local model in this wave; only network calls.

## Exit criteria

1. `/answer?q=` streams tokens within 1 s of the first provider chunk through Bifrost.
2. An answer with zero sources is shown with a visible "ungrounded" notice and is not
   cached (test).
3. CI AI smoke green; full offline eval scores at or above baseline.

## Steps

### W4-01 OpenAI streaming client and `/models`
- Issue #50 · Effort M · Label feature · Team Systems · Branch `v3/w4-01-openai-client`
- Depends on: W2-07, W3-06
- Do: `oxe-core::ai::openai`: SSE-streamed chat completions with tool-call delta
  assembly, usage extraction, typed errors (auth, rate limit with retry-after, context
  length), `GET {base_url}/models` listing with a 60 s cache; `wiremock` fixtures recorded
  from Bifrost for tests; metrics `oxe_ai_requests_total{model,outcome}`,
  `oxe_ai_tokens_total{model,kind}`, `oxe_ai_duration_ms`; audit row per provider call
  (model, tokens, ms, request_id), never the prompt text.
- Acceptance: fixture-driven test assembles a two-chunk tool call correctly; a 429 fixture
  surfaces `retry_after_s`.
- Follow-up: W4-02.

### W4-02 Tool loop and grounded-only caching
- Issue #51 · Effort M · Label feature · Team Systems · Branch `v3/w4-02-tool-loop`
- Depends on: W4-01
- Do: `stream_answer(query) -> impl Stream<AnswerFrame>`: system prompt (cite `[n]`,
  metadata tail JSON), loop with `search_web` tool over the pipeline, `sources` collected
  from tool results (deduped, in citation order), tail parsing tolerant (v2's
  `parse_final_answer` semantics, rewritten), `done.confidence`; `answers` table + `Store`
  methods; cache hit replays `sources` then `done{cached:true}`; `ungrounded=true` flag in
  `done` when no sources.
- Acceptance: replay + recorded transcript yields 2 `step` frames, deltas, `sources` with
  2 URLs, `done`; transcript with no tool calls yields `ungrounded=true` and no `answers`
  row; provider error mid-stream yields partial deltas then `error`.
- Follow-up: W4-03.

### W4-03 Answer SSE route and HTMX answer page
- Issue #52 · Effort M · Label feature · Team Product Builders · Branch `v3/w4-03-answer-page`
- Depends on: W4-02
- Do: `POST /api/answer` (SSE), `/answer?q=` page: steps as a progress line, streamed text,
  citation markers linking to source cards, related questions as links, `confidence`,
  `cached`, `ungrounded` notice, `request_id`; the search page gets an "ask" link to
  `/answer?q=` when `ai.enabled`; when disabled the page says so and links to `/settings`.
- Acceptance: page test on replay + transcript sees text appear before `done`; the
  ungrounded notice renders for the no-tool transcript.
- Follow-up: W4-04.

### W4-04 AI evals: offline set and CI smoke
- Issue #53 · Effort M · Label infra · Team Systems · Branch `v3/w4-04-ai-evals`
- Depends on: W4-03
- Do: `oxe eval ai evals/ai/*.jsonl` (offline, transcripts + cassettes, parallelism from
  available cores), scoring: cited-domain recall, `must_contain`/`must_not_contain`
  string checks, ungrounded rate; results to `evals/results/<date>-ai.json`; baseline and
  tolerance in `evals/thresholds.toml`; CI fast gate runs the 5 cases tagged `smoke` and
  fails only if below baseline minus tolerance; `oxe eval ai --record` regenerates a
  transcript against the live provider (owner-run).
- Acceptance: CI job time < 30 s; a deliberately broken tail parser drops the score below
  baseline in a test.
- Follow-up: W4-05.

### W4-05 Anthropic Messages protocol
- Issue #54 · Effort M · Label feature · Team Systems · Branch `v3/w4-05-anthropic`
- Depends on: W4-04
- Do: `oxe-core::ai::anthropic`: Messages API streaming with tool use blocks, mapped onto
  the same `AnswerFrame` stream; `protocol = "anthropic"` selects it; fixtures recorded via
  Bifrost's Anthropic passthrough or the public API; evals run against both protocols when
  transcripts exist.
- Acceptance: the W4-02 tests pass with the Anthropic fixtures.
- Follow-up: W5-01.

## Out of scope for W4

Local models, embeddings, multi-turn chat, answer sharing.

## Follow-up

W5 `wave-5-archive-and-semantic.md`.
