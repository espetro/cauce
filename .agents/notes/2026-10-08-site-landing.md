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
- Screenshots in `src/assets/` are REAL captures of v0.8.1 served locally
  (musl binary, `XDG_DATA_HOME` scratch dir, `localStorage cauce-theme` for
  dark). Retake on each visual release; the answer shot is a real
  `/answer/{id}` render (RRF answer log row).
- Design tokens mirror `crates/cauce-server/assets/style.css`
  (--bg/--fg/--accent, light default + prefers-color-scheme dark). If the app
  theme changes, update `site/src/app.css`.
- `site/` is a new top-level dir not in the v3 plan's §4.1 layout — flag to
  owner; if the plan is updated, add it there.
- No live demo exists yet: the search box and the "durable /answer/{id} link"
  degrade to same-origin/self-host hints until `VITE_CAUCE_ORIGIN` is set.
