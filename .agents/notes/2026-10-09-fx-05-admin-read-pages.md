# 2026-10-09 — FX-05 admin + read pages in the SPA (#266)

- Ported `/history`, `/dashboard`, `/settings`, `/archive` and merged the
  three ops surfaces into one `/app/admin` tabbed route (`engines`,
  `cache`, `audit`) — no standalone HTMX pages remain for any of them.
- Wire contract: `GET /api/config` **redacts** secret leaves as
  `<redacted>` — the settings view edits the redacted shape and the
  server restores real values on PUT. `api_key` never leaves the box.
- `routes_table_matches_plan_section_6` reads §6 of
  `2026-09-21-v3-rust-core.md` via `include_str!` — every
  backtick-quoted `/path` token in a row parses as a mounted route, so
  prose mentions of removed pages must not use backticks. Cargo does
  NOT track `include_str!` deps: touch a .rs file in the same target to
  force a rebuild after editing the plan.
- The HTMX shell (header.html) keeps nav tiers but every read/admin
  link now points at `/app/*` deep links (`/app/admin?tab=engines`
  etc.); `ui_shell` tests assert those hrefs appear twice
  (`.nav-operator` + `.nav-more`).
- Dead-code removal: `templates/{history,dashboard,settings,archive,
  archive_markdown,archive_markdown_error,audit,cache,cache_payload,
  cache_payload_error,engines,engine_card,settings_cache}.html`,
  `src/{cache_page,dashboard,engines_page}.rs`, `src/html/{archive,
  history,settings}.rs`, `web/src/engines.ts` (the old classic-bundle
  engines module — its enable/disable/reset calls moved into the SPA).
- `DELETE /api/cache` + `DELETE /api/cache/{key}` were already in the
  wire; admin cache tab uses both (clear-all + per-key).
