# SPEC: oxe web UI redesign (manual + AI search)

Date: 2026-09-15
Status: active

## Goal

Redesign the oxe web UI to support two search modes from one codebase:

1. **Traditional search** (current): card-less Google-anatomy results, cache
   transparency, shareable URLs.
2. **AI search** (new, v0.3+): answer-first streaming-style view in the
   Morphic / Perplexity / Vane direction, with cited source cards and related
   questions.

## Hard constraints (from oxe architecture)

- Single Python process, stdlib-only HTTP server; server-rendered HTML.
- Vanilla JS only for progressive enhancement (no framework, no build step).
- Dark-mode friendly: `color-scheme: light dark`, Geist embedded.
- Same URL content-negotiates: HTML to browsers, Exa JSON to
  `Accept: application/json` clients.
- Low memory baseline target; no SSR framework, no node runtime.
- Cache transparency must remain visible (hit/miss, backend, duration,
  q_hash prefix).

## Reference directions

- miurla/morphic (9.1k, MIT) — generative UI, streaming answer, related questions.
- ItzCrazyKns/Vane (36.8k, Apache-2.0) — answer engine layout.
- felladrin/MiniSearch (587, MIT) — browser-local minimalism.
- Mobbin patterns (Perplexity, Exa, Sana AI): oversized centered search input,
  generous whitespace, answer-first hierarchy, compact horizontal source
  cards, related-question list rows.

## Deliverables for phase 1 (design)

ASCII screen specs in `.agents/docs/screens/`, format:
intro / `## ASCII mockup` / `## Behavior` / `## Responsive` / `## Notes`,
extended with `## User flow checkpoints` (entry state → query → streaming →
citations → follow-up → history). Screens: landing, traditional results,
AI answer, plus any shared states (empty/error). Must cover base states +
both search modes without clutter.

## Deliverables for phase 2 (tech stack)

Writer+reviewer loop choosing the SSG/CSR stack for the UI layer, given the
fixed design requirements above. Must justify against: stdlib-only server,
zero build step preference (or minimal), memory footprint, dark mode,
streaming answer rendering, progressive enhancement.

Final stack decision (rev 2): `.agents/drafts/tech-stack.md` — Preact +
Vite CSR, daisyUI 5 on Tailwind 4, `mise.toml` tooling. Screen specs
built against it live in `.agents/docs/screens/`.

- Phase loop: designer/QA and writer/reviewer each get max 5 iterations.
- QA scores 0-10; threshold 8. Below 8: apply feedback or restart fresh.
- Fixed artifacts land in `.agents/docs/screens/` (design) and
  `.agents/drafts/tech-stack.md` (stack decision).
