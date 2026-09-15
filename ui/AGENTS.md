# ui/ — agent guidelines

Scope: everything under `ui/` (Preact + Vite webapp). Repo-wide rules live in the root `AGENTS.md`; screen specs in `.agents/docs/screens/` are the behavioral source of truth.

## Design tokens

- Design token system: daisyUI (on Tailwind CSS 4). Everywhere else in these docs, "design tokens" refers to this system. If it is swapped, only this one line changes.
- NEVER write vanilla HTML styling. Every surface uses design tokens (daisyUI component classes + theme custom properties). Raw `btn`, `card`, `input`, `tabs` primitives come from the token system; project components compose them.
- Prefer premade design-token components as-is. Only build a custom component when:
  1. the needed logic doesn't exist in the token set, or
  2. inlining token classes becomes too verbose/repeated across files.
  In that case, modularize: one component, built ON TOP of token components, in `ui/src/components/`.

## Architecture

- File-based routing: `ui/src/routes/` mirrors URLs (`index.tsx` → `/`, `search.tsx` → `/search`, `history.tsx` → `/history`). One route file per spec screen.
- Feature-driven grouping where a feature spans multiple components: `ui/src/features/<feature>/` (e.g. `suggests/` owns the dropdown + hook + endpoint client).
- Components follow SRP: one component = one job. `<SearchBox>` does not fetch; `<SuggestionsDropdown>` does not submit; data flow is props down, callbacks up.
- Shared primitives live in `ui/src/components/`; feature-specific ones stay inside their feature folder. Do not hoist prematurely.

## Hard constraints (from the stack decision)

- No-JS is not required; keep client payload small (budget: 40KB gz JS / 30KB gz CSS, enforced by `mise run check`).
- Content negotiation is backend behavior; the UI always speaks HTML/routes.
- Dark mode via token theme (`color-scheme: light dark`); Geist self-hosted. No other fonts.
- Streaming: `AnswerStream` consumes chunked fetch; citation markers `[n]` render as superscript links targeting `<SourceCard>` ids.

## Responsiveness

- Every screen must work at ≈390px, 768px, and desktop. Mobile-first styling.
- Before declaring UI work done: screenshot at mobile viewport and compare against the spec's `## Responsive` section.

## Quality loop

- `mise run lint` (oxlint + oxfmt) and `mise run check` (size budgets) must pass.
- UI polish standard: high-end visual design per `.agents/docs/screens/` specs; when in doubt, fewer boxes, more whitespace, card-less anatomy.
- Deliberately constrained flexibility: don't invent alternate layouts, extra dependencies, or CSS outside tokens. Go straight to the point.
