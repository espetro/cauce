# W7: AI mode UX — first-class AI search + Search Assist

Iteration 8. Priority P1. Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`.
Previous: `wave-6-postgres-and-multi-instance.md`. Research basis: issue #198.

## Goal

Make AI mode a first-class surface instead of a link buried on the results page: a
prompt-first AI mode with conversation threads, and a compact single-turn Search Assist
card on the SERP that answers from the already-returned result set without an extra engine
fetch.

## Settled inputs

- `GET /answer` + `POST /api/answer` SSE stream exist (W4): frames `step`, `delta`,
  `sources`, `done`, `error`; raw `fetch` + `ReadableStream` pump (EventSource can't POST).
- `AnswerLoop` (`cauce-core::ai::answer`) already runs the ReAct tool loop over
  `search_web` + `search_archive` and emits `confidence`/`ungrounded`/`related_questions`.
- `AnswerBody { q }` is `deny_unknown_fields` — new params must be declared.
- `ChatRequest` supports `tools` + `tool_choice` — a no-tools assist turn needs no provider
  changes.
- Grounded-only cache rule (W4): `answers` written only with ≥1 source, `confidence >= 4`,
  no error. Assist answers reuse this rule; the key must incorporate the result-set hash so
  an assist answer for `q` never collides with a tool-loop answer for the same `q`.
- Research consensus (issue #198): assist is on-demand by default — never auto-run on every
  SERP; always-visible source chips; "auto-generated — may contain inaccuracies"
  disclaimer; AI-off means every AI entry point hidden or greyed.

## Exit criteria

1. From `/` or the SERP, a user reaches a streaming `/answer` in one interaction, and the
   AI entry is in primary nav.
2. A SERP "Assist" click streams an answer grounded in the shown results with no engine
   re-fetch (verified: zero outbound engine requests during assist).
3. The path taken is visible: 'answered directly' vs 'searched web/archive' plus the
   confidence value render on the answer surface.
4. `/answer` supports a follow-up turn with prior context and per-turn sources.

## Steps

### W7-01 First-class AI entry points
- Issue #199 · Effort S · Label feature · Team UI · Branch `v3/w7-01-ai-entry`
- Depends on: W4-03 (merged)
- Do: `/answer` in `nav-primary` (new `strings::common` key, `AnswerPage.nav_active =
  "answer"`); an 'AI' mode affordance on the search box (Google AI Mode pill pattern —
  mode switch inside the form, reusing the submit hijack in `templates/page.html`; no
  second input); entry points hidden/greyed when `state.answer().is_none()`.
- Acceptance: one interaction from `/` starts a streaming answer; disabled state renders
  correctly.
- Follow-up: W7-02.

### W7-02 Search Assist panel on the SERP
- Issue #200 · Effort M · Label feature · Team UI · Branch `v3/w7-02-search-assist`
- Depends on: W7-01
- Do: `AnswerBody.context_results: Option<Vec<AnswerSource>>`;
  `AnswerLoop::stream_assist(req, results)` — no tools, `tool_choice: "none"`, results
  serialized into the prompt, sources → deltas → done; cache key includes top-K result
  hash; assist card in `templates/page.html` above results, on-demand 'Assist' trigger
  (waits for `meta` frame on `?stream=1` pages); chrome = label, short answer,
  always-visible source chips (favicon + domain), "auto-generated — may contain
  inaccuracies", 'Ask in AI mode' handoff link.
- Acceptance: assist streams a grounded answer citing on-page results; zero engine calls;
  hidden when AI disabled.
- Follow-up: W7-03.

### W7-03 Surface retrieval path + confidence
- Issue #201 · Effort S · Label feature · Team UI · Branch `v3/w7-03-confidence`
- Depends on: W7-02 (badge also applies to the assist panel)
- Do: emit/render a step line recording the path taken ('answered directly — no search
  needed' vs 'searched web'/'searched N sources' vs archive); confidence badge + existing
  `ungrounded` notice rendered on `done`, on `/answer` and inside the assist panel.
- Acceptance: a no-tool answer shows 'answered directly' + confidence; a searched answer
  shows which tools ran — visible in-stream, not post-hoc.
- Follow-up: W7-04.

### W7-04 Conversation threads on `/answer`
- Issue #202 · Effort L · Label feature · Team UI · Branch `v3/w7-04-threads`
- Depends on: W7-01
- Do: multi-turn — `AnswerRequest`/`AnswerBody` carry conversation state (client-passed
  `messages` or server thread id; pick the simpler that keeps the 5-iteration cap and
  grounded-only caching correct — cache latest turn only, or skip cache on multi-turn);
  turn list UI on `/answer` (user bubble + streamed reply + per-turn sources), bottom
  pinned follow-up input; follow-ups keep tool access.
- Decision: ephemeral page-state threads for v1; a `threads` table (resumable threads,
  history listing) is follow-up, not this wave.
- Acceptance: `/answer?q=` → answer → follow-up → second turn streams with prior context;
  each turn's sources attach to that turn.

## Out of scope for W7

- DDG-style long-form assist expansion and preset follow-up questions on the SERP
  (follow-up work once the base card lands).
- Assist frequency setting (often/sometimes/on-demand/never) and thumbs feedback.
- Thread persistence across reloads (`threads` table), thread list in history.
- Model/effort picker in the AI input.
- MCP `answer` tool.

## Risks

- Assist that reuses the tool loop instead of a no-tools variant wastes a provider turn and
  can rephrase the query — the `context_results` path is the point of the step.
- Auto-running assist on every search is the documented anti-pattern (Google AIO
  backlash); on-demand default is settled.
- Ungrounded or low-confidence output must say so (`ungrounded` notice, low badge) — never
  confabulate; honor W4's grounded-only cache rule.
