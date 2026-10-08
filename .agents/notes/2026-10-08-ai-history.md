# 2026-10-08 — AI-mode history: origin labels + durable answer log (#254)

## What landed (branch `v3/ai-history`)

- Migration `0005_ai_history.sql`: `search_log.origin` (`'user'` default backfill) +
  append-only `answer_log` (no TTL; `status` done|cached|error).
- `SearchOrigin { User, Agent }` in cauce-core; `SearchRequest.origin` is
  `#[serde(skip)]` server-owned — `SearchWeb::execute` marks it, `resolved_origin()`
  (tool-marked OR non-Ui client => agent) is applied in `write_log`.
- `Store::log_answer` returns the new row id; `get_answer_log(id)` fetches it.
  `AnswerLoop::run` logs every terminal path (fresh/cached/error, multi-turn too);
  `stream_assist` does NOT log.
- `HistoryItem::Answer` merged in `list_history` (honors since/q/origin; `cached=1`
  excludes answers); `history_stats` counts both kinds.
- `/history` page defaults `origin=user`; `/api/history` JSON stays unfiltered.
  `HistoryFilter.origin: Option<SearchOrigin>`, `?origin=user|agent|all` on both.
- `GET /api/answer-log/{id}` (JSON) + `DELETE /api/answer-log/{id}` (audited
  `answer_log.delete`), both wave 2.
- `GET /answer/{id}` (wave 4, `ui` gated): static server render of the stored row
  via `render_answer_html` — question, sources, related, meta chips, error text;
  `ctx.not_found` on missing id. Follow-up form intentionally not rendered
  (no thread context — resumable threads remain a follow-up).
- `AnswerFrame::Done`/`Error` carry `log_id: Option<i64>` (additive, ts-rs regen);
  `/answer?q=` JS `replaceState`s to `/answer/{log_id}` on terminal frames, so
  back/forward lands on the static render instead of re-POSTing the agent loop.
  Cached replays write a fresh `'cached'` row so the id always lands on a real row.

## Traps hit

- `/history` tests seed `client=api` rows → all derive `origin=agent` → the new
  page default (`user`) emptied every fixture. Sed'd tests to `?origin=all` rather
  than rewriting fixtures; page code probes the unfiltered store on empty pages so
  the empty-state copy stays correct.
- `Store::log_answer` signature change (`()` → `i64`) ripples to StubStore,
  observability test stubs and every agent call site.
- `rows::answer_log` returns `Result<_, StoreError>` — `query_row` closures need
  `.map_err(rows::as_sql)`.
- `mise run web` freshness check is a bare `git diff --exit-code` on generated
  paths — it FAILS on legitimate uncommitted generated changes. Stage the files
  (`git add`) before running it; the test run dies as collateral when web fails.
- `render_answer_html` lives at `cauce_core::ai::render_answer_html`, not the root.
- `Path<i64>` in `answer_view` vs `Path<String>` in JSON handlers — axum parses
  both; string parse keeps the "replay" 400-probe convention.
