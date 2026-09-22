# later: Postgres store, auth, and multi-instance deployment
Issue: #61, #62, #63

Seam: the `Store` trait plus `cauce-core::conformance` (W0-04, hardened in W6-01), which is
parameterised over a connection URL precisely so a second impl can plug in.
Trigger: an actual second-store or multi-instance deployment demand. Until then SQLite is
the only shipped impl and non-loopback binds refuse to start (W1-13).

Postgres `Store` impl (former W6-02): `crates/cauce-store-postgres` via `sqlx`
(runtime-checked queries, no compile-time DB), migrations under the crate, `pgvector` for
tier 3, `tsvector` + `ts_rank_cd` for lexical, `expires_at` index for eviction, an advisory
lock so one instance runs the eviction worker, `store.url = "postgres://..."` dispatch in
`cauce-cli`, `cauce store migrate` for both backends, and a CI job with a Postgres service
running the conformance suite (including a two-instance shared-cache case).

Admin auth (former W6-03): static bearer tokens in config
(`[auth] admin_tokens = ["${env:CAUCE_ADMIN_TOKEN}"]`) required for `DELETE /api/cache*`,
`PUT /api/config`, `POST /api/engines/*/reset`, `/metrics` and `/audit` when
`auth.enabled` (default false on loopback, forced true when bind is not loopback); routes
table entries carry `auth: admin|none`; audit rows record the token id (hash prefix),
never the token. This item replaces W1-13's non-loopback refusal with real auth.

Deployment docs (former W6-04): `docs/deploy/` for launchd (oxmgr), systemd, and Docker
Compose (cauce + Postgres + optional OTel collector + Grafana dashboards JSON for the W1-09
metrics), plus a `docs/agents.md` refresh and a README section on the multi-instance
story.
