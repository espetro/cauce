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
- Dark mode via token theme (`color-scheme: light dark`); fonts: Plus Jakarta Sans Variable (body), Apfel Grotesk (logotype, self-hosted), system fallbacks. No Geist.
- Streaming: the answer view consumes chunked fetch; citation markers `[n]` render as superscript links targeting `<SourceCard>` ids.

## Responsiveness

- Every screen must work at ≈390px, 768px, and desktop. Mobile-first styling.
- Before declaring UI work done: screenshot at mobile viewport and compare against the spec's `## Responsive` section.

## Design references

- Any image the owner shares as a design reference MUST be copied to
  `.agents/docs/screens/references/` (gitignored) with a descriptive filename
  (e.g. `ddg-dark-pill.png`, `google-ai-mode-morph.png`), then cited from the
  relevant screen spec in `.agents/docs/screens/<screen>.md`.
- Before implementing or validating UI work, ALWAYS check that folder first:
  it accumulates the owner's reference designs (search engines, UI patterns)
  and is the single place where design references live for every agent,
  including subagents. Do not rely on paths under /var/folders or ~/Documents
  surviving between sessions.

## URL state & QA checkpoints

URL-addressable state is the app's reproducibility contract: every meaningful
UI state should be deep-linkable. Full checkpoint list, status, and rationale
lives in `.agents/docs/screens/userflow-checkpoints.md`.

Param contract:

| Param | Values | Screen/state |
|---|---|---|
| `q` | query text | `/search?q=` Search-mode results (works today) |
| `p` | page number; **absent = page 1** | **deprecated** (continuous scroll): deep links with `p` are ignored/stripped; generated links never carry it |
| `mode` | `ai` (absent = Search) | AI answer view (works today); unavailable AI stays on Search results with an inline notice |
| `settings` | `open` / `close` (absent = closed) | settings dialog, valid on any route (works today; stripped on close/save) |
| `since` | `24`/`168`/`720`/`all` | history time filter (works today) |
| `qf` | substring | history query-text filter (works today) |
| `suggest` | `1` (+`q`) | suggestions dropdown open, QA-only (planned) |
| `force` | `error`/`ai-off`/`empty` | stub error/notice/empty states, QA-only (planned) |

`suggest=1` and `force=*` are planned/questionable: they are listed so QA
agents know the intended contract, but they do not work yet and must not
be relied on until implemented. `since` and `qf` work today
(`routes/history.tsx`); `all` (absent param) is the default and is
stripped from the url. Theme and mode persist
in `localStorage` (`oxe-theme`, `oxe-mode`), deliberately not URLs.

State library: custom hooks on top of preact-iso's `useLocation()` /
`route()` (pattern: `routes/search.tsx` mode handling, `components/Header.tsx`
settings handling). No new state dependencies (`qss`, signals, nanostores
are all unnecessary at this size); JS budget stays 40KB gz.

QA agent convention: (a) drive states via URL deep links, not
click-throughs, whenever a URL recipe exists; (b) when you find a new key
checkpoint while testing, name it, make it reproducible via URL state
(propose or implement the param), and update both the table above and
`.agents/docs/screens/userflow-checkpoints.md` — this convention is the
contract; keep the two lists in sync.

## Quality loop

- Dev port: `mise run dev` runs the backend on **4480** (must match the vite
  proxy target in `ui/vite.config.ts`); the production/preview server is
  **4479**. When testing against a server you started yourself, check which
  port it's on before blaming CORS/502s — a stale instance on the other port
  is the usual culprit.

- `mise run lint` (oxlint + oxfmt) and `mise run check` (size budgets) must pass.
- UI polish standard: high-end visual design per `.agents/docs/screens/` specs; when in doubt, fewer boxes, more whitespace, card-less anatomy.
- Deliberately constrained flexibility: don't invent alternate layouts, extra dependencies, or CSS outside tokens. Go straight to the point.
- Icons: unplugin-icons with Lucide set (`~icons/lucide/*`), no inline SVGs, no other icon sets.
