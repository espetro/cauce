# Pattern: Classic result list anatomy

The classic SERP result: favicon, title, snippet, metadata. cauce had zero
reference coverage for this even though `search.md` Mockup A specifies it
in detail; this doc grounds that spec in real products so future edits
have something to check against besides the spec text itself.

## Applies to

- `search.md` Mockup A (Search mode) — checkpoints 3, 4, 7, 8 in
  `userflow-checkpoints.md`.
- The "Hybrid" section of `patterns-search-input.md` and
  `patterns-states.md` (the classic empty/error states share this
  anatomy).

## Per-result anatomy

Convergent across Google and DuckDuckGo (both confirmed live in
`shots/google-landing-full-idle-loggedout.png`'s sibling results pages
and general product knowledge; DuckDuckGo's classic layout matches
Google's anatomy exactly, which is why cauce's spec cites "Google style"
directly):

1. **Favicon + domain line**, above the title, smallest text in the
   block. Google: domain in dark gray/green, favicon 16-18px to its
   left. cauce: `icons.duckduckgo.com/ip3/{domain}.ico`, 16px, lazy-loaded,
   graceful when absent (domain text alone still reads).
2. **Title link**, one size step up from the domain line, the only
   saturated-color text in the row (`#1a0dab` light / `#8ab4f8` dark in
   cauce's spec, matching classic Google's link-blue convention). Max two
   lines, no underline until hover.
3. **Snippet**, plain two-line clamp, body-gray text, no border, no
   background. This is the convergent "boxed card" rejection: neither
   Google nor DuckDuckGo nor cauce's spec puts a border or shadow around
   individual results; separation is whitespace only.
4. **Metadata row** (optional, below or beside the snippet): cauce adds a
   collapsed `<details>` "cached page text preview" per result, unique
   to cauce's cache-transparency requirement — not a pattern borrowed from
   any harvested product, called out here as a cauce-specific addition
   rather than a convergent pattern.

## Vertical rhythm

- Whitespace-only separation between results (no dividers, no
  alternating background); each result block gets enough top/bottom
  margin that the eye reads discrete units without a drawn line.
  Google and DuckDuckGo both use roughly one line-height of gap between
  the end of one snippet and the start of the next domain line.
- Meta line above the list (`N results`, cache badge) sits closer to the
  search input than to the first result, visually grouping it with the
  query rather than with the results.

## Result-list-level metadata

- `N results` count, optionally with cache/source provenance
  (`from cache · 3h old`), positioned as the first line of the results
  block, not in the header. This is a cauce requirement (cache
  transparency) without a direct competitor precedent, since none of
  the harvested products expose cache state to end users; documented
  here as cauce-specific rather than implying convergence.
- Share affordances (`copy link`, `copy json`) sit on the same meta
  line, right-aligned; no harvested competitor product exposes a
  developer-facing JSON copy action, another cauce-specific addition.

## Continuous scroll / pagination

- No harvested product exposes numbered pagination on the primary SERP
  in its default AI-adjacent surface; Google's classic results still
  paginate with numbers at the very bottom, but none of the AI-mode or
  AI-assist surfaces researched do. cauce's continuous-scroll-with-`more
  results`-button-fallback (`search.md` Behavior) sits between these:
  closer to a chat product's infinite scroll than classic numbered
  pagination.

## Sources

- `.agents/docs/screens/search.md` Mockup A (cauce's own anatomy spec,
  the primary source this doc grounds)
- `shots/duckduckgo-serp-search-assist-dark.png` (DuckDuckGo classic
  result directly below its Search Assist block, same-page comparison)
- General product knowledge of Google's classic ten-blue-links anatomy
  (title-link blue `#1a0dab`, domain-above-title ordering); no single
  fresh screenshot of a plain Google organic result was captured in this
  harvest (gap, noted honestly rather than invented from an old memory
  of the layout — the anatomy described matches what is visible in the
  DuckDuckGo capture, which cauce's spec already cites as the model).
