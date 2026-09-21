# Pattern: AI Answer Streaming

How AI search sites render answers progressively and cite sources.

## Applies to

Checkpoints 9, 10, 11, 13, 14 in `userflow-checkpoints.md` (AI answer
streaming, done with sources, cached hit, mid-stream failure, empty
sources). Governs `search.md` Mockup B (streaming) and Mockup B2
(completed).

## Phase model

Perplexity's canonical three phases, shown as a status pill (13px, muted, rounded-full, 8px 12px padding):
1. Searching (amber accent)
2. Reading / Reading sources (blue)
3. Writing (green)

Each phase swap updates the status line text ("Searching the web...", "Reading 3 sources..."). Escalate copy if a phase exceeds 2s: "Still working, this can take up to 10s."

## Order of reveal (critical)

1. User query echoes into transcript immediately (<100ms, optimistic).
2. Skeleton in answer slot within 100ms: 2-3 grey rounded lines of varying width matching expected answer shape. Shimmer/pulse cycle 300-700ms.
3. Source cards fade in BEFORE answer text (Perplexity: citations are "ready" by the time you read past them).
4. Answer streams token-by-token with a blinking block cursor at the tail (Perplexity uses a teal block cursor).
5. Related/follow-up questions render last.

Never buffer and render once. Parse markdown progressively; hold back a trailing still-forming fragment until it resolves (avoids garbage from half-open code fences).

## Streaming contract (from smoothui.dev / ai-tldr)

- First visible pixel < 400ms (Doherty threshold): paint shell + thinking indicator before first token.
- A caret is ALWAYS visible during an active stream; no caret reads as broken.
- Stop button active for the whole stream, backed by a real AbortController; after stop, keep partial text and show a "Generation stopped" label + regenerate.
- Lock submit during streaming; input re-enables the instant the stream ends.
- Auto-scroll follows the tail, pauses the moment the user scrolls up, resumes on send.
- Announce streaming region with `aria-live="polite"` and `aria-busy="true"` while generating.

## Citations

- Inline numbered superscript tokens `[1]` typed INTO the answer text, not appended chrome. Perplexity: 12px, weight 500, accent color, bg accent/10, radius 4px, padding 2px 6px.
- Citation sits at the point of the claim, not at paragraph end.
- Hover/tap opens a small source popover (favicon 16px + domain 11-13px uppercase mono + title 14px, 2-line clamp). Open in 120ms.
- Below the answer: horizontal "Sources" strip of cards (favicon + domain + title). Full source list collapsed by default ("Reviewed sources" expandable).
- Each citation link gets `aria-label="Source 1: example.com"`.

## Layout

- Answer column max-width ~680px (~68ch), left-aligned in a centered page.
- Sources rail may scroll horizontally above/beside the answer; never interleaved with prose.
- Follow-up suggestions as chips (pill radius, 14px) below sources.

## Sources

- https://blakecrosley.com/guides/design/perplexity
- https://unpkg.com/oh-my-design-cli@1.9.0/web/references/perplexity/DESIGN.md
- https://skills.smoothui.dev/docs/ai-chat
- https://ai-tldr.dev/learn/building-ai-apps/ai-ux-patterns/designing-for-llm-latency/
- https://frontendpatterns.dev/guides/managing-ai-response-states
- https://9to5google.com/2025/03/05/google-search-ai-mode-announcement/
