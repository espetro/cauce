# ui/ frontend rules

One approved answer per problem. React is brittle precisely because it offers six ways to hold
state; the alternatives in the right column below are lint errors or missing dependencies, not
code-review comments. Full rationale in
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` ("The frontend rules (commit 1, not
commit 40)").

## Approved vs. banned

| Problem | Approved | Banned |
|---|---|---|
| Routing | TanStack Router, file-based, `loader()` for data | any `useEffect` fetch |
| URL state | `validateSearch` with a valibot schema | hand-parsed `useSearchParams` |
| Screen state | `useReducer` over a discriminated union, exhaustive `never` default | `useState` chains for multi-step flows |
| Global state | `@xstate/store`, hard cap of 3 stores | context providers, jotai, redux |
| Server state | router `loader()`; SSE into a rung-3 reducer | `useEffect` + `setState` |
| Wire types | `openapi.json` -> `openapi-typescript` -> `openapi-fetch` | hand-written response interfaces |
| Looks | daisyUI class names | raw Tailwind layout soup, custom CSS files |
| Behavior | `@base-ui/react` parts | hand-rolled focus traps, portals, comboboxes |
| Copy | Lingui macros, catalogs under `src/locales/` | any literal string in JSX |
| Icons | `unplugin-icons` + Lucide | inline SVG, other icon sets |
| Memoization | React Compiler | manual `useMemo` / `useCallback` |

Approved libraries and their pinned versions live in `ui/package.json` and `ui/.oxlintrc.json`;
this document links to them by name and does not restate versions.

## The state ladder

Pick the highest rung that can hold the state. Going lower requires a note in the PR.

1. **URL.** Anything a QA agent should reach by link. Typed by `validateSearch`.
2. **Other non-opaque state.** `localStorage` behind a valibot codec, uncontrolled inputs via
   `FormData`, router loader cache.
3. **`useReducer` FSM.** Discriminated union state, exhaustive `never` check, pure reducer.
4. **`@xstate/store`.** Capped at 3 stores.

## The daisyUI / Base UI doctrine

If it is looks, it is a daisyUI class; if it is focus, keyboard or portal, it is a Base UI
part. They compose on the same element via Base UI's `render` prop.

## Lingui rules

No literal strings in JSX. Copy goes through Lingui macros against catalogs under
`src/locales/`.

## Enforced

- `useEffect` is banned outside `src/lib/effects/`: oxlint's `no-restricted-imports` rule
  blocks importing `useEffect` from `react`, overridden off only inside that directory.
  [gate: ui/.oxlintrc.json]
- oxlint's native React Compiler correctness rules run as errors (`"plugins": ["react"]`,
  `categories.correctness: "error"`) — rules-of-hooks, purity, immutability, refs,
  set-state-in-effect, preserve-manual-memoization, and friends. [gate: ui/.oxlintrc.json]
- TypeScript runs in strict mode with `noUncheckedIndexedAccess`, over both `src/` and
  `scripts/` (guard scripts are typechecked too, not exempted). [gate: ui/tsconfig.app.json]
- The build (`extract -> compile -> vite build -> tsc -b`) and the lint pass both run in the
  merge gate. [gate: validate]
- React Compiler memoization is enabled at the transform level
  (`jsc.transform.reactCompiler: true` in `ui/vite.config.ts`), so components opt into
  compiler-managed memoization by default. This is the transform being on, not a lint rule
  banning manual `useMemo`/`useCallback` — see Conventions. [gate: ui/vite.config.ts]
- The JS/CSS gzip size budget is measured and enforced by `ui/scripts/size-budget.ts`, runnable
  via `bun run size` after a build. Numbers live only in that file — see it for the current
  ceiling/target/floor, do not restate them here. [gate: ui/scripts/size-budget.ts]

## Conventions

- The rest of the approved/banned table — `validateSearch` for URL state, `useReducer` FSM for
  screen state, `@xstate/store` (cap 3) for global state, router `loader()` for server state,
  `openapi-fetch` for wire types, daisyUI for looks, `@base-ui/react` for behavior, Lingui for
  copy, `unplugin-icons` for icons — describes the target architecture. None of daisyUI,
  `@base-ui/react`, `@xstate/store`, valibot, or the OpenAPI codegen pipeline are installed yet
  (wave 2/3 work per the plan); there is nothing to gate until they land, and no code exists
  yet that could violate these rules.
- "No literal strings in JSX": Lingui is installed and one message is wired end to end, but
  there is no lint rule today banning a literal JSX string. Convention until such a rule (or an
  equivalent test) exists.
- "No manual `useMemo`/`useCallback`" is not a banned-import lint rule the way `useEffect` is;
  the React Compiler transform being on is Enforced (above), but nothing today fails the build
  if a component still hand-writes `useMemo`.
- The size budget script exists and can be run by hand, but is not yet wired into `mise run
  validate` or a `validate:full` task (that task doesn't exist yet) — running it is a
  convention until it's part of a gate.
