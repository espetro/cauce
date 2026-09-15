# nosql-sqlite.md applicability to oxe

**Date:** 2026-09-15
**Source doc:** `/Users/josocjoq/Documents/nosql-sqlite.md`
**Project:** `/Users/josocjoq/Documents/prjcts/_own/oxe`
**Mode:** research (no code changes)

## TL;DR

The findings doc's premise ("DDL migration overhead is the dominant pain for embedded schemas")
does not apply to oxe. The project has had exactly **one** ALTER TABLE in its lifetime
(`ALTER TABLE cache ADD COLUMN created_at` at `cache.py:31-33`), and it is already handled
idempotently with a 3-line try/except. No other column has been added to any table. The
hybrid SQLite-JSONB pattern in the doc would add cost with no offsetting benefit. None of
the full-engine swaps (LMDB, RocksDB, PoloDB, Sled, SurrealDB) preserve oxe's load-bearing
properties: 70 MB RSS budget, single-Python-process pitch, aiosql-driven `.sql` files,
WAL-mode read-only cross-process stats (`oxe stats`), and SQL idioms the suggest/dashboard
features depend on (`GROUP BY query_hash`, `LIKE … ESCAPE`, `COUNT / SUM / MIN`).

**Net applicability: low. Recommend no action.** Defer until a concrete schema-less need
appears (e.g. AI tool-call metadata with unknown shape), and even then keep it inside
SQLite via a narrow `documents(collection, key, data JSON, expires_at)` table — do not
reach for a new engine.

---

## 1. Workload profile

Quantified from oxe source (paths cited inline; full table in research notes):

| Op | Table | R/W | Per /search | Hot path | Lock |
|---|---|---|---|---|---|
| `get_cache` | `cache` (PK) | R | 1 | yes | none |
| `hits_bump` | `cache` | W | 0–1 | yes | none |
| `put_cache` + expiry GC | `cache` | W | ~0.5 | yes | `_lock` |
| `log_search` | `search_log` | W | **1** (unconditional) | yes | `_lock` |
| `record_click` | `clicks` | W | ~0.05–0.5 | cold | `_lock` |
| `suggest_queries` | `search_log` GROUP BY + LIKE ESCAPE | R | 0–1 (per keystroke) | yes (debounced) | none |
| `get_clicks` LEFT JOIN cache | `clicks`↔`cache` | R | 0–2 | warm | none |
| `stat_*` (GROUP BY) | `search_log` | R | 0 (offline `oxe stats` only) | none | none |

- Cache payload: gzip-json of Exa result list, ~2–8 KB/row, ≤30 results.
- `search_log`: writes per `/search` are **unconditional** (`search.py:34,51`); retention
  30 days → ~30k–150k rows (~3 MB packed). This is the only "append-heavy" table.
- `clicks`: ~50/day single-user, capped at 30-day retention → ~1.5k rows.
- `cache` size: bounded by unique queries in last `OXE_TTL_MAX=86400s` window.
- The DB is roughly **<5%** of `/search` wall time. DDG scrape (~10s timeout) dominates.

## 2. Fit matrix (oxe tables × options)

| oxe table | LMDB | RocksDB | PoloDB | Sled | SurrealDB-embedded | SQLite-JSONB |
|---|---|---|---|---|---|---|
| `cache` (PK get, expiry scan, COUNT stats) | ✓ | △ | △ | △ | ✗ | ✓ |
| `answers` (same shape as cache) | ✓ | △ | △ | △ | ✗ | ✓ |
| `clicks` (write log + LEFT JOIN to cache) | △ | △ | ✓ | △ | △ | ✗ |
| `search_log` (write log + 3× GROUP BY + prefix LIKE ESCAPE) | △ | △ | ✓ | △ | △ | ✗ |

**Reading the matrix:**
- `cache`/`answers` are point-get → opaque-blob workloads → KV-shape (LMDB) or JSONB shape fit.
- `clicks` requires a real `LEFT JOIN` to `cache`; collapsing both into a `documents`
  table breaks the join, and a JSONB column with virtual indexes doesn't recover it.
- `search_log` has three different `GROUP BY`s and a prefix-LIKE-ESCAPE; each needs a
  generated column + index in a JSONB plan, which is more code than the current SQL
  definitions, with no upside.

## 3. Engine-by-engine assessment

| Option | New deps | RSS impact | Stats rewrite | Threading rewrite | `oxe stats` RO-mode preserved? | Verdict |
|---|---|---|---|---|---|---|
| **LMDB** (`lmdb` 2.3.0) | +1 | **+50–200 MB** (mmap counts as RSS) | full (all SQL → Python scans) | minor | **no** (no cross-process RO mode) | ✗ exceeds RSS budget |
| **RocksDB** (`rocksdict` 0.3.29) | +1 | **+50–150 MB** (memtable + block cache floor) | full | minor | no | ✗ exceeds RSS budget |
| **PoloDB** (`polodb-python`) | +1 | +5–15 MB | full (no SQL JOIN, no LIKE ESCAPE) | minor | partial | △ bus-factor of 1 maintainer |
| **Sled** | 0 | n/a | full | n/a | n/a | ✗ **no maintained Python binding**; would require building one |
| **SurrealDB embedded** | +1 (heavy) | **+100 MB** (engine is full DB server in-process) | medium (SurrealQL port) | yes (sync↔async) | partial | ✗ beta, ~50–100 MB binary, wrong scale |
| **Hybrid SQLite (JSONB)** | 0 | 0 | partial | none | **yes** (same SQLite file) | △ wins on no-deps, loses on workload fit |
| **Stay on SQLite** | 0 | 0 | none | none | yes | ✓ |

**Hard blockers across the non-SQLite options:**
1. **RSS budget** (`MEMORY.md`, `plans/2026-09-15-v0.4.0-web-ui-ai.md:20`) — the 70 MB RSS
   pitch is a load-bearing product claim; mmap (LMDB) and LSM (RocksDB) blow it.
2. **`oxe stats` RO-mode** (`stats.py:18-22`, `.agents/decisions.md:1`) — read-only URI
   mode is the canonical way for an operator dashboard to read while the proxy writes.
   LMDB/RocksDB require coordination that defeats the workflow.
3. **aiosql has no bindings** for LMDB, RocksDB, PoloDB, or Sled. Engine swap means
   deleting `oxe/sqlload.py` + 5 `.sql` files + rewriting 28 named queries + 25 DB tests.
4. **aiosql is in the runtime deps** (`pyproject.toml:47`). The "minimal runtime
   dependency surface" rule (`oxe/AGENTS.md:7-8`) means swapping engine costs a dep
   (aiosql stays or goes) **and** an addition (the new engine) — every commit needs
   justification in the message.
5. **`oxe/AGENTS.md:21` rule** — *"Schema changes go through oxe/sql/ migration files;
   never hand-edit a live DB."* LMDB/RocksDB/PoloDB/Sled make this literally impossible.
6. **`group-by` workloads** in `search_log.sql:21-23, 25-27, 37-38` and prefix-LIKE in
   `search_log.sql:29-35` are doing real work today. Pure-KV pushes every aggregation
   to Python over full keyspaces.

## 4. The doc's pain point — verified against oxe

**Claim:** *"Replacing an embedded relational schema with a schema-less or key-value
store eliminates table-migration overhead."*

**Evidence in oxe:**
- **One DDL migration in the whole project.** `cache.py:31-33`:
  `try: ALTER TABLE cache ADD COLUMN created_at INTEGER except OperationalError: pass`.
- **Every DDL statement is `IF NOT EXISTS`.** `schema.sql:7,15,16,25,26,28,39,40,42,50`.
- **No migration runner, no version table, no per-version files.** `executescript(
  sqlload.schema_sql())` at `cache.py:29` runs on every boot; idempotent.
- **0 column additions to `clicks`, `search_log`, or `answers` ever.** `created_at` is
  the only historical addition.
- Other `OperationalError` handlers (`stats.py:28-35`, `stats.py:295`) are lock/DB
  guards, not migration pain.

**Verdict:** the doc's migration-friction argument does not generalise from "lots of
evolving columns" to "zero evolving columns since launch." The single 3-line ALTER
already in place proves oxe's pattern scales to any churn the project will see.

## 5. The "Hybrid SQLite-JSONB" pattern — narrowest viable slice

The doc proposes:
```sql
CREATE TABLE documents (collection, key, data JSON, expires_at,
  PRIMARY KEY (collection, key)) WITHOUT ROWID;
CREATE INDEX idx_doc_expiry ON documents(collection, expires_at);
```
with virtual columns exposing nested fields.

**Verdict: don't do this today.** Reasons:

1. **Current `cache.response` is already a gzip+JSON blob.** The hybrid pattern wins
   when you want to query subfields; oxe doesn't — `set`/`get`/`hits_bump` operate on
   the whole payload (`cache.py:46-51, 53-65`). Replacing BLOB with `data JSON` would
   add SQLite-side JSON parse cost on every read for zero benefit.
2. **`get_clicks` LEFT JOIN** (`clicks.sql:6-9` to `cache.query_text`) is real product
   behaviour. Collapsing both into `documents` breaks the JOIN; keeping them split
   defeats the consolidation argument.
3. **`search_log` GROUP BY columns** (`query_hash`, `client`, `backend`,
   `result_count`, `duration_ms`) each become a generated column — strictly more code
   than today's SQL.
4. **5 generated columns + 4 indexes** would be needed to recover the lookups that
   currently work via real columns (full list in research notes). Net cost: higher.
5. **Atomic `hits_bump` regresses.** `UPDATE … SET hits = hits + 1` becomes
   `UPDATE documents SET data = json_set(data, '$.hits', json_extract(...) + 1)`.
   Two concurrent writers no longer have a single atomic UPDATE.

**When this slice would be reasonable:** if a future feature needs to ingest truly
schema-less payloads (e.g. AI tool-call metadata of unknown shape), add a new
`documents` table for that feature alone — **do not** migrate the existing 4 tables.

The minimal delta in that future case is ~30–50 lines + ~5 tests in a new
`tests/test_documents.py`. Until that need arrives, the right move is **no action.**

## 6. Migration cost scorecard

| Target | `.oxe/` files to change | Approx. lines | Tests to rewrite |
|---|---|---|---|
| Stay on SQLite (do nothing) | 0 | 0 | 0 |
| Hybrid SQLite-JSONB | 2 (`schema.sql`, `cache.sql`/`cache.py`) | 30–60 | ~5 in `test_cache_sql.py` |
| LMDB / RocksDB / PoloDB / Sled | **10–13** (drop `sqlload.py` + all 5 `.sql`; rewrite `cache.py`, `server.py` 14 sites, `mcp_server.py` 4 sites, `stats.py` full file, …) | ~800–1500 | **all 25** DB tests + rewrite `oxe stats` aggregations in Python |

## 7. Where the findings *could* inspire future work (without leaving SQLite)

- **Generated columns for `cache.hits` and `search_log.duration_ms`** if a future
  dashboard wants fine-grained slicing — purely additive, no schema migration, lives
  in `schema.sql`.
- **`jsonb(...)` on `cache.response`** to enable ad-hoc SQL queries over the cached
  Exa payload without leaving SQLite — strictly opt-in, no impact on the hot path
  if columns are not queried.
- **Migrate the `created_at` ALTER out of `cache.py:31-33`** into `schema.sql:13` —
  removes the inline migration wart that "motivated" the doc. One-line cleanup, no
  scope, no new dep.

## 8. Recommendation

Do not adopt any of the six options in the doc today. The doc's central premise
(migration overhead) does not describe oxe's reality; every full-engine swap
contradicts at least one of (RSS budget, aiosql-driven `.sql` files, RO-mode
cross-process reads, lean-deps policy); the hybrid-SQLite path fits only `cache`/
`answers`, not `clicks`/`search_log`, and adds cost without offsetting gain.

Reassess when (and only when) one of these is true:
- A real schema-less use case appears (e.g. arbitrary AI tool-call metadata).
- One of the four tables needs a high-volume write path that SQLite WAL can't serve
  (current data: ~1 search_log INSERT/search, ≤150k rows/30d — well within SQLite).
- 70 MB RSS budget is explicitly relaxed by the project owner.

Until then, the narrowest changes worth making are the three opt-in ideas in §7
above; each is local to `oxe/sql/` and `oxe/cache.py`, respects the AGENTS.md rules,
and can be merged without breaking the `oxe stats` RO-mode contract or the aiosql
dependency.

---

**Subagent outputs referenced:**
- per-table fit mapping (LMDB / RocksDB / PoloDB / Sled / SurrealDB / SQLite-JSONB)
- dependency & risk audit (PyPI versions, wheels, license, concurrency, packaging)
- workload profile, RSS budget analysis, migration path cost
