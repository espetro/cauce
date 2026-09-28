# `cauce-agent` extraction (#230, PR #237)

- Boundary landed: `ChatProvider` + the chat/answer wire vocab (`ChatRequest`,
  `ChatCompletion`, `ChatMessage`, `ToolCall`, `ToolSpec`, `AiStreamEvent`,
  `AnswerFrame`, `AnswerRequest`, `AnswerTurn`) **stay in `cauce-core`** — they are the
  settled contract `evals/ai.rs` (`score_frames`, `RecordingProvider`) and the server's
  SSE layer already consume. Everything that *runs* the loop moved to
  `crates/cauce-agent`: `AnswerLoop`, `LoopConfig`, `ToolExecutor`/`ToolCtx`/
  `ToolOutput` registry+dispatch, `AgentObserver`/`LoopEvent`/`RunContext`.
- Cyclic dev-dep is intentional and documented: `cauce-agent` deps on `cauce-core`;
  `cauce-core` dev-deps back on `cauce-agent` for the moved `tests/answer*.rs`. The
  `score_frames` → `AnswerFrame` link is why the vocab could not move.
- `run_assist` now folds through the same private `turn()` helper as the tool loop —
  one provider-call/timeout/delta path instead of two.
- **Hook model mirrored rig-core's `AgentHook`**: single `on_event` + run-scoped
  context, reduced to observe-only (no `Flow` return — nothing consumes control flow,
  so hooks stay zero-cost and can't alter the loop). `TracingObserver` reproduces the
  exact prior `answer.stream`/`answer.assist` spans + cache-failure warns via
  `run_span(&RunContext)` + `LoopEvent`s (Started/Finished/ProviderTurn/
  ProviderCompleted/ProviderFailed/ToolStarted/ToolFinished/CacheFailed/Frame);
  `NoopObserver` is the default. `LoopEvent::Frame` fires on frame *production*
  (before `tx.send`), documented — "produced, not delivered".
- `tail_parser` is a `fn(&str) -> (String, u8, Vec<String>)` field on `LoopConfig`
  (default `parse_final_answer`) so a third-party harness can swap the final-answer
  contract without forking the loop.
- Orphan rule: `ChatProvider` impls for the two provider clients live in
  `cauce-core/src/ai/mod.rs` (trait + impl in one crate); `cauce-agent` re-exports the
  trait and only ever holds `Arc<dyn ChatProvider>`.
- Live e2e notes (evidence on PR #237): `openrouter/free` never converges the tool
  loop (tools every turn → max-iterations error frame); `openai/gpt-4o-mini` converged
  on first try via `CAUCE_AI_MODEL=` override. Cassette hit requires matching the
  *model's* search phrasing, not the user's question.
- Devin-box gotcha: shell git is a function wrapper; `DEVIN_COMMIT_AUTHOR=` is the
  supported way to force a specific author email (env/gitconfig config is forced to
  the espetro noreply otherwise). `--author`/`-c user.*` alone get overridden.
- Follow-up per the issue: split budgets / dedup / forced-synthesize land *inside*
  this crate — the seam is where a third-party harness (rig-core, genai) plugs in.
- Owner direction on autonomy: the harness should be ReAct-shaped (model decides
  each step; loop = orchestration + safety bounds), not a fixed pipeline. `run()`
  already is — provider turn → tool dispatch → repeat until tool-free completion or
  `max_iterations`/`provider_budget`. `run_assist` stays single-turn by contract
  (inline grounding, no tools). Documented in lib.rs "Loop shape"; #231 deepens the
  guards inside the same iteration.
