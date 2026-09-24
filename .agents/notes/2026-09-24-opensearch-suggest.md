# OpenSearch suggestions endpoint — issue #150 (2026-09-24)

Findings from implementing `GET /api/suggest` (`v3/opensearch-suggest`).

## No frecency machinery existed — `Store::suggest` is new

The issue said the Store trait "already has history/frecency machinery".
It does not: `list_history` is a merged newest-first feed and nothing
ranks queries. The trait got a minimal extension instead —
`suggest(prefix, limit) -> Vec<String>` over distinct
`search_log.query` values. "Frecency" is implemented as
`ORDER BY COUNT(*) DESC, MAX(ts) DESC` (frequency primary, most-recent
use the tiebreak) — worth knowing if a real frecency scorer lands later;
the conformance check pins the ordering semantics, not the formula.

## Prefix matching is `normalize_query` + SQLite `LIKE` + escaping

`LIKE` is ASCII case-insensitive by default — not enough on its own,
since `search_log.query` is written via `normalize_query` (unicode
lowercase + whitespace collapse). Devin Review caught the gap:
`?q=Éclair` or `?q=éclair  ` missed `éclair recipes`. `SqliteStore::
suggest` now runs the prefix through `normalize_query` first — the
trait doc makes that part of the contract so every impl folds user
input the same way stored queries were written; the raw term still
echoes back in the response. `like_prefix` sits next to `like_pattern`
in `store/mod.rs` and does the same `%`/`_`/`\` escaping plus one
trailing wildcard; keep using `ESCAPE '\'` in the SQL or the escapes
mean nothing.

## Wire-shape details the tests pin

- Completions echo the normalized `query` column, not `query_raw` —
  deterministic output (the conformance check seeds a row whose
  `query_raw` differs in casing to lock this in).
- Blank/whitespace `q` echoes `""`, not the raw whitespace —
  `["", []]` covers absent, `?q=`, and `?q=%20`.
- The handler short-circuits blank `q` without a store call; the sqlite
  impl also defensively returns `[]` for empty prefix or `limit == 0`.

## Routes-table bookkeeping

Registered wave 2 with `requires: None` — it is the W2-11 descriptor's
promised endpoint, a wave-2 omission like the favicon. Adding a route
touches four places: `app.rs` match arm, `routes.rs` `ROUTES`, the
plan's section-6 table, and `EXPECTED_*` lists in
`tests/routes_table.rs`. Miss any and a different test catches it.
