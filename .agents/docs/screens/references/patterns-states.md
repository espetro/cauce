# Pattern: Empty, Loading, Error, Rate-Limit States

## Applies to

Checkpoints 3, 5, 7, 8, 9, 10, 12, 13, 14, 15 in
`userflow-checkpoints.md` (results loading/rendered, zero results, backend
error, AI streaming/done, AI unavailable, mid-stream failure, empty
sources, history empty state). Governs `search.md` Mockup C and the
loading affordances in Mockups A/B, plus `history.md`'s empty state.

## Empty state (pre-search)

- Hero input centered + 3-6 suggested starter prompts as pills below (14px, pill radius, subtle border). ChatGPT/Perplexity pattern: suggestions teach the input format.
- Keep it calm: no illustration overload, one sentence of guidance max. First-run empty state can be generous; returning users skip straight to input.
- Duck.ai lesson: make history discoverable here (recent chats / pinned) so the empty state doubles as a resume point.

## Loading

- Skeleton < 100ms after submit, shaped like the expected answer (2-3 grey lines, varying widths). Skeletons feel ~20% faster than spinners for identical waits; use a spinner only for <300ms waits.
- Skeleton pulse/shimmer cycle 300-700ms; colors ~5% alpha ink, never saturated.
- Multi-step work gets a progressing status line ("Searching...", "Reading 3 sources..."), not a static skeleton. A 15s silent skeleton reads as frozen.
- Thinking > 2s: escalate to explicit duration copy.
- Reserve space (min-height) so skeleton-to-text causes no layout shift; never imply more structure than the answer will have.

## Stopped (user cancelled)

- Distinct terminal state: keep partial text, subtle "Generation stopped" label, offer Regenerate. Re-enable input immediately.

## Error

- Inline in the answer slot, never a vanishing toast. Specific cause + Retry button (+ copy-error-id if applicable).
- Mid-stream failure: preserve received tokens, mark interrupted, offer Continue or Regenerate.
- Model refusal/moderation is normal output (message styling), not a red error.
- 429 / rate limit: calm, distinct treatment: "Rate limit reached, retrying in Ns" with countdown or disabled Retry until cooldown. Exponential backoff. Separate from network errors in copy.
- Context-length errors get their own message (retry will not help).

## State machine

Name every state explicitly: idle, submitted, thinking, streaming, complete, stopped, error. Track per message/turn. The skipped states (thinking, stopped) are exactly the ones users notice.

## Accessibility

- Thinking state in a polite live region; streamed text replaces it. `aria-busy="true"` while generating.

## Hybrid (classic + AI)

The classic SERP's empty/error states are NOT the AI answer's empty/error
states, they are deliberately simpler, per `search.md` Mockup C:

- **Classic zero results**: one line, `no results`, plus an
  `ask AI instead` escape hatch shown only when AI mode is available. No
  illustration, no retry framing (a zero-result classic query is not an
  error, it is a fact about the query).
- **Classic backend error**: `error: search failed: 502` plus `retry` and
  `ask AI instead` buttons on the same row. Distinct copy from a
  zero-result state: this IS an error, framed as one.
- **AI empty sources**: a different failure shape than classic zero
  results: `no sources found for this query - try fewer words, or
  [view Search]`. The AI answer can still theoretically generate text with
  no grounding sources; cauce's contract treats sourceless AI answers as a
  failure state rather than rendering an ungrounded answer, so this reads
  as an error-adjacent state even though the model itself did not error.
- **AI mid-stream failure**: partial answer text is kept on screen, the
  cursor is replaced by `stream interrupted - [retry] [view Search]`. This
  has no classic-mode analogue (a classic result-list fetch either
  succeeds or hits the backend-error state above; there is no partial/
  interrupted classic result list by design, since results either arrive
  as a complete page or don't).
- **The shared escape hatch is asymmetric**: `ask AI instead` only appears
  from classic states (routing INTO the richer mode on failure), while
  `view Search` only appears from AI states (routing OUT to the simpler,
  more deterministic mode on failure). Neither state offers a retry into
  the opposite mode's success case; both offer a lateral move to the
  other mode's idle-submit path.
- **AI mode unavailable** is its own third bucket, neither empty nor
  error: Search results render normally with a small muted notice under
  the pill (`AI mode is not configured - set a model in settings`) and
  the AI segment disables in place. No competitor in the harvested set
  needs this state (their AI surface is either always-on or a fully
  separate product), so it is an cauce-specific addition, not
  pattern-derived.

## Failure naming and stale results

- **Name the failed source, stay silent on success** (SearXNG's collapsed
  source-messages area). cauce has one upstream, so the failure line names the
  engine and error class (`error: search failed: 502 (ddg)`) and nothing
  renders when the fetch worked.
- **Empty copy differs by position** (SearXNG). First page with no results
  says so and offers an action (`ask AI instead` when AI is available);
  end of a continuous-scroll list says `end of results` with no action. Both
  use `role="alert"`-equivalent polite announcement, not a toast.
- **Stale cache on throttle** (cauce-specific, from the DuckDuckGo verdict:
  the backend throttles with 403 or CAPTCHA, so this is a first-class
  failure). When a fetch fails and a cached result set for the same query
  exists, show it with an age notice (`cached 3h ago, refresh failed`)
  instead of the bare error. Not built yet: the service treats expired rows
  as misses, so `search.md` lists it under queued improvements and the
  evaluator does not check it.

## Sources

- https://frontendpatterns.dev/guides/managing-ai-response-states
- https://ai-tldr.dev/learn/building-ai-apps/ai-ux-patterns/designing-for-llm-latency/
- https://skills.smoothui.dev/docs/ai-chat
- https://insideduckduckgo.substack.com/p/duck-tales-improving-ai-chat-organization
- https://blakecrosley.com/guides/design/perplexity
- `search.md` Mockup C (cauce's own hybrid empty/error contract)
