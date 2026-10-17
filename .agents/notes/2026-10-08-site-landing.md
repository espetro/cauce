# site/ landing page (2026-10-08)

Added `site/` — a standalone Svelte 5 + Vite + TS static landing page for
cauce, deployed to Cloudflare Pages (build `site/`, output `dist/`). It is NOT
part of the workspace or `mise run validate` gates — it has its own
`pnpm install && pnpm check && pnpm build` and its own lockfile.

Design: direction "product page that is the product" (research in
`~/cauce-landing-research.md`): working search-box hero (form submits to
`VITE_CAUCE_ORIGIN` + `/search`, or same-origin `/search` when unset so it
works when cauce itself serves the page), real product screenshots instead of
fake divs, three-surface split (UI/API/MCP), grounded-answer section, engine
YAML teaser, self-host table, honest self-host-vs-hosted comparison.

Traps:

- `svelte-check` does NOT run on typescript@7 (tsgo has no `ts.sys`); the site
  pins `typescript ~5.9.3` deliberately — the web/ TS7 toolchain does not carry.
- Script-less `.svelte` components error in svelte-check
  ("Could not find a declaration file") — keep an empty
  `<script lang="ts"></script>` in every component.
- Screenshots in `src/assets/` are REAL captures of the post-FX-06 SPA era
  (debug build of main, `CAUCE_DATA_DIR`/`CAUCE_CONFIG_DIR` scratch dirs,
  playwright-core over CDP for dark emulation): `/app/search` results
  dark+light and a real grounded `/answer/{id}` render. Retake on each visual
  release.
- Design tokens mirror `crates/cauce-server/web/src/spa/app/app.css`
  (--bg/--fg/--accent, light default + prefers-color-scheme dark — same
  values the HTMX style.css used). If the app theme changes, update
  `site/src/app.css`.
- `site/` is a new top-level dir not in the v3 plan's §4.1 layout — flag to
  owner; if the plan is updated, add it there.
- No live demo exists yet: the search box and the "durable /answer/{id} link"
  degrade to same-origin/self-host hints until `VITE_CAUCE_ORIGIN` is set.
