# W6: Postgres store and multi-instance

Iteration 7 (2026-12-15 to 2026-12-28). Priority P2. EPIC issue: #7.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-5-archive-and-semantic.md`.

## Goal

The enterprise swap the architecture promised: a second `Store` implementation on Postgres
(`pgvector`, `tsvector`) that passes the same conformance suite as SQLite, several `oxe`
instances sharing it, authentication on the admin and metrics surfaces, and deployment docs.
This wave proves the trait boundary; it does not build a hosted product.

## Settled inputs

- `Store` conformance suite from W0-04 (`oxe-core::conformance`) is the contract; both impls
  run it in CI (Postgres via `services:` container in the workflow, not in the local fast
  gate).
- Postgres via `sqlx` (runtime-checked queries, no compile-time DB), migrations in
  `crates/oxe-store-postgres/migrations/`, `pgvector` for tier 3, `tsvector` + `ts_rank_cd`
  for lexical, `expires_at` index for eviction, advisory lock for the single eviction
  worker across instances.
- Selection by URL: `store.url = "sqlite:///path"` (default) or `postgres://...`; cargo
  feature `postgres`.
- Auth: static bearer tokens in config (`[auth] admin_tokens = ["${env:OXE_ADMIN_TOKEN}"]`)
  required for `DELETE /api/cache*`, `PUT /api/config`, `POST /api/engines/*/reset`,
  `/metrics` and `/audit` when `auth.enabled` (default false on loopback, forced true when
  bind is not loopback). Nothing more elaborate in v3.0.

## Exit criteria

1. Conformance suite green on Postgres in CI.
2. Two `oxe serve` processes against one Postgres share cache hits and one runs eviction.
3. `docs/deploy/*.md` cover launchd (oxmgr), systemd, Docker Compose (with Postgres).

## Steps

### W6-01 Store conformance suite hardening
- Issue #60 · Effort M · Label infra · Team Systems · Branch `v3/w6-01-conformance`
- Depends on: W5-05
- Do: promote the W0-04 conformance module to cover every `Store` method including
  `get_semantic`, `audit`, `health`, `pages`, `answers`; concurrency cases (two writers,
  eviction during reads); TTL boundary cases; runs against SQLite in the fast gate and is
  parameterised over a connection URL for W6-02.
- Acceptance: suite green on SQLite; a deliberately broken `get_lexical` fails one named
  case.
- Follow-up: W6-02.

### W6-02 Postgres `Store` implementation
- Issue #61 · Effort L · Label feature · Team Systems · Branch `v3/w6-02-postgres-store`
- Depends on: W6-01
- Do: `crates/oxe-store-postgres` per settled inputs; `store.url` dispatch in `oxe-cli`;
  eviction worker with advisory lock; CI job with a Postgres 17 + pgvector service running
  the conformance suite; `oxe store migrate` command for both backends.
- Acceptance: exit criteria 1 and 2 (the two-instance test runs in the Postgres CI job
  only).
- Follow-up: W6-03.

### W6-03 Auth on admin and metrics surfaces
- Issue #62 · Effort S · Label feature · Team Systems · Branch `v3/w6-03-auth`
- Depends on: W1-12
- Do: bearer-token middleware per settled inputs; routes table entries carry `auth:
  admin|none`; `bind` non-loopback without `auth.enabled` refuses to start with a clear
  error; audit rows record the token id (hash prefix), never the token.
- Acceptance: routes-table test asserts every mutating route is `admin`; a non-loopback
  bind test without tokens fails startup.
- Follow-up: W6-04.

### W6-04 Deployment docs
- Issue #63 · Effort S · Label documentation · Team Product Builders · Branch `v3/w6-04-deploy-docs`
- Depends on: W6-02, W6-03
- Do: `docs/deploy/launchd-oxmgr.md`, `systemd.md`, `docker-compose.md` (oxe + Postgres +
  optional OTel collector + Grafana dashboards JSON for the W1-09 metrics), `docs/agents.md`
  refresh, `README.md` rewrite with the three modes and the zero-effort integrations.
- Acceptance: a fresh macOS or Linux VM follows one doc end to end (owner or agent run,
  recorded in `.agents/notes/`).
- Follow-up: v3.0 release checklist (`later/release-v3.0.md`).

## Out of scope for W6

Multi-tenancy, per-user quotas, SSO, hosted instance hardening, proxy pools/OHTTP.

## Follow-up

`later/` items and the v3.0 release checklist.
