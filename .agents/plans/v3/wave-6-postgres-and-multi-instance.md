# W6: store conformance

Iteration 7 (2026-12-15 to 2026-12-28). Priority P2. EPIC issue: #7.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-5-archive-and-semantic.md`.

## Goal

The enterprise-readiness deliverable is the conformance suite itself: `oxe-core::conformance`
hardened to cover every `Store` method and parameterised over a connection URL, so a second
`Store` implementation is a config change when one is actually demanded. The Postgres impl,
admin auth, and deployment docs are deferred to `later/postgres-and-multi-instance.md`.

## Settled inputs

- `Store` conformance suite from W0-04 (`oxe-core::conformance`) is the contract; the suite
  is parameterised over a connection URL so a second impl can plug in later.
- The `Store` trait is the seam; `store.url` selects the impl (`sqlite:///path` is the only
  shipped one).

## Exit criteria

1. Conformance suite green on SQLite and parameterised over a connection URL ready for a
   second impl.

## Steps

### W6-01 Store conformance suite hardening
- Issue #60 · Effort M · Label infra · Team Systems · Branch `v3/w6-01-conformance`
- Depends on: W5-03
- Do: promote the W0-04 conformance module to cover every `Store` method including
  `get_semantic` (a trait stub until `later/semantic-tier.md`), `audit`, `health`,
  `pages`, `answers`; concurrency cases (two writers,
  eviction during reads); TTL boundary cases; runs against SQLite in the fast gate and is
  parameterised over a connection URL ready for a second impl.
- Acceptance: suite green on SQLite; a deliberately broken `get_lexical` fails one named
  case.
- Follow-up: v3.0 release checklist (`later/release-v3.0.md`).

## Out of scope for W6

Multi-tenancy, per-user quotas, SSO, hosted instance hardening, proxy pools/OHTTP.
Postgres `Store` impl, admin/metrics auth, and deployment docs are deferred to
`later/postgres-and-multi-instance.md` (issues #61, #62, #63).

## Follow-up

`later/` items and the v3.0 release checklist.
