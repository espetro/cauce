# Pattern: Micro-Animations and Motion

## Applies to

Checkpoints 5, 9, 10, 13 in `userflow-checkpoints.md` (search loading,
AI streaming, AI done, AI mid-stream failure) plus the theme toggle
(checkpoint 18/19's settings dialog) and the suggestions dropdown
(checkpoint 6). Governs motion in `search.md` Mockups A/B/B2 and
`landing.md`'s suggestions dropdown.

## Duration tokens (Perplexity motion ladder, widely matched)

| Token | ms | Use |
|---|---|---|
| instant | 0 | Toggle flips, pill selection |
| fast | 120 | Hover, focus rings, button press, citation popover |
| standard | 220 | Card hover-lift, popover open, tab switch, source expand |
| slow | 360 | Modal/dialog entrance, drawer slide |
| stream | per-token | Answer streaming cadence (never artificial pacing slower than real tokens) |

Easing: ease-out for entrances, ease-in for exits, ease-in-out or `cubic-bezier(0.4, 0, 0.2, 1)` for moves. Entrances shorter than exits where they differ.

## Signature motions

1. Token streaming with block cursor: text appears token-by-token in reading order, cursor trails the last token, cursor blinks ~1s cycle. Under reduced motion, collapse the stream to a single fade-in of the completed answer.
2. Skeleton shimmer: background-position sweep or opacity pulse, 300-700ms loop, ease-in-out.
3. Source card hover-lift: border warms to accent + lift shadow (`0 2px 8px rgba(0,0,0,0.08)`), 150-220ms ease-out. Resting shadow minimal (`0 1px 2px rgba(0,0,0,0.05)`).
4. Sources collapse/expand: height auto-animate ~220ms standard easing; chevron rotates 180deg same duration. Full source list expands on demand, collapsed by default.
5. Citation popover: opens in 120ms with light backdrop blur; fast enough to feel "always there".
6. Phase indicator: label crossfade (150-200ms opacity) between Searching/Reading/Writing; avoid layout jumps by fixing pill width or animating width 220ms.
7. Cursor/caret blink during stream: opacity step at ~530ms (standard caret rate).

## Theme transitions (light/dark)

- Color transitions on background/text/borders at 150-200ms ease when toggling theme. Do NOT transition everything (`*`) - scope to color properties to avoid janky layout interpolation.
- Perplexity dark mode shifts accent brighter (`#20808D` -> `#34B4C4`); keep accent legibility per theme rather than one literal.

## Anti-patterns

- Fake typewriter pacing slower than the actual stream (fast model feels slow).
- Auto-scrolling while the user reads earlier content.
- Removing the stop button mid-stream.
- Full answer revealed in one flash (skips the trust-building stream).

## Reduced motion

`prefers-reduced-motion: reduce`: streaming becomes a single fade, cursor blink stops, all durations drop to 0-80ms, shimmer becomes static. Product stays fully usable, just static.

## Sources

- https://unpkg.com/oh-my-design-cli@1.9.0/web/references/perplexity/DESIGN.md
- https://skills.smoothui.dev/docs/ai-chat
- https://ai-tldr.dev/learn/building-ai-apps/ai-ux-patterns/designing-for-llm-latency/
- https://blakecrosley.com/guides/design/perplexity
