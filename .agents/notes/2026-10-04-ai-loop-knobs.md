# 2026-10-04 — `[ai]` answer-loop knobs (#233, branch v3/ai-knobs, PR #248)

- `AiConfig` gained `max_turns` (8), `max_searches` (6),
  `provider_budget_s` (60, validated >=1), `verify` (false, reserved for
  #232's groundedness verifier — parsed + hot-reloaded, NOT consulted and
  NOT on the settings form). TOML `max_searches` maps to
  `LoopConfig::max_search_executions` (issue spelling won the public name;
  internal name stays #231's).
- `AiConfig` needed a manual `Default` impl (nonzero defaults) — same
  pattern `CacheConfig` used for SWR. Every struct-literal `AiConfig {…}`
  in tests then needed `..Default::default()`; ~9 sites across
  cauce-core/cauce-server test files.
- `provider_budget_s = 0` rejected in `from_raw` (degenerate: every call
  times out instantly; `ai.enabled = false` is the off switch).
  `max_turns`/`max_searches` = 0 stay legal.
- `eval_ai` (cauce-cli) deliberately left on `LoopConfig::default()` —
  transcript-replay mode never loads Config; determinism beats parity.
- Hot-reload verified end-to-end: saving Max turns=3 via `/settings` →
  next `/api/answer` ran 3 turns + forced synth, no restart (`ai.*` rides
  the #236 `commit_locked` → Runtime rebuild path for free).
- Testing-skill additions landed in the same PR: count `span_open`
  records by kind (substring grep overcounts ~17x via `spans` parent
  arrays), unique-query-per-call stub = the sharp `max_searches` proof,
  `tool_choice:"none"` body dispatch = forced-synthesize turn.
- Drive-by: `api_report_days_bounds_the_window` fixture was hardcoded to
  `cauce-2026-09-28.jsonl` and rotted out of its `days=1` window —
  `write_logs` now dates files at `Utc::now()` (separate commit). Watch
  for more wall-clock-rotted fixtures; `within_days` computes the cutoff
  from now.
