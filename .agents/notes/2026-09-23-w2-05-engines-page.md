# 2026-09-23 — W2-05 engines page

- Page/data-plane pattern settled: `/engines` and `GET /api/engines` share ONE handler
  (`handlers::engines_list`); `Accept: text/html` renders the Askama page, everything else
  JSON. Same for the card's test query: `GET /api/search` under `Accept: text/html` returns
  the `search_fragment.html` partial (meta line + shared `results.html` include). No bespoke
  UI endpoints — negotiated arms on the shared handlers is the wave-2 idiom.
- HTMX error arm discipline: fragment errors are answered 200 for `HX-Request` callers so
  htmx swaps the error-class meta line in place (a real 4xx/5xx skips the swap); direct
  `Accept: text/html` callers still see the true status (`ApiError::status()` exposed for it).
- Reset semantics changed (W2-05 acceptance): `HealthTracker::reset` now lands on
  `HalfOpen` (next call is the single probe), not `Closed`. Updated core health tests, CLI
  e2e `blocked_replay_opens_skips_and_survives_restart`, audit details carry `from`/`to`.
- Enable/disable is two endpoints `POST /api/engines/{id}/enable|disable` (the screen spec
  replaced the plan's `/{id}/enabled?enabled=`; plan route table updated). Both patch the
  raw pre-interpolation TOML tree like `PUT /api/config` (templates survive), synthesize a
  `[[engines]]` entry for built-ins/spec-registered engines absent from the file, audit
  `engine.enable`/`engine.disable`, and report `effective_after_restart: true`.
- `strings.rs` convention: `crate::strings::common` + one `pub mod <page>` of flat consts,
  referenced from templates as `crate::strings::engines::X` (Askama resolves it). W2-04 and
  W2-05 each append their own module; merge order decides who resolves the file conflict.
- Card view splits: `engines_page.rs` (page + card templates, string shaping) vs
  `html.rs` (landing/search + shared assets `HTMX_JS`/`JSON_ENC_JS`/`STYLE_CSS`,
  `accepts_html`/`is_htmx`/`render_err` helpers). Don't grow html.rs per page.
- Rebase note: main's `RouterOptions` (html.rs) and OpenSearch/favicon route expectations
  (tests/routes.rs) conflicted with W2-05's additive changes; resolution was keeping both
  sides, never dropping main's newer arms.
