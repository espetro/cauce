# oxe UI

Vite 8 + React 19, transformed end to end by SWC (no Babel): React Compiler via
`jsc.transform.reactCompiler` and Lingui via `@lingui/swc-plugin`, both in the same
`jsc.experimental.plugins` pass configured in `vite.config.ts` (see `scripts/swc-options.ts`
for the shared constants). Routing is TanStack Router, file-based, under `src/routes/`.

See `ui/AGENTS.md` for the approved-versus-banned state and tooling table once it lands.

## Commands

Run everything through `mise` from the repo root, or `bun` directly from `ui/`:

- `mise run dev:ui` / `bun run dev` - dev server
- `mise run lint:ui` / `bun run lint` - oxlint, including the native React Compiler
  correctness rules and the `useEffect` ban outside `src/lib/effects/`
- `mise run tsc` / `bunx tsc -b` - typecheck (`strict` + `noUncheckedIndexedAccess`)
- `mise run build:ui` / `bun run build` - extract and compile Lingui catalogs, `vite build`,
  then typecheck
- `bun run test` - vitest, including the build-time proof that the SWC pipeline composes
  (`scripts/verify-swc-pipeline.test.ts`)
