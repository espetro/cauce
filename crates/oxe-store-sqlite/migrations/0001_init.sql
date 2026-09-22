-- W0-04 initial schema (parent plan section 5).
--
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at <https://mozilla.org/MPL/2.0/>.
--
-- All timestamps are INTEGER unix epoch milliseconds. `*_json` columns hold
-- compact JSON text. `cache_vec`, `answers` and `pages` are deliberately
-- absent: they land in later waves.

-- Tier-1 exact cache. Single writer: Store::put / evict_expired / admin deletes.
CREATE TABLE cache_entries (
    key          TEXT PRIMARY KEY,
    query        TEXT NOT NULL,
    params_json  TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    hits         INTEGER NOT NULL DEFAULT 0,
    engines_json TEXT NOT NULL
);
CREATE INDEX cache_entries_expires_at ON cache_entries (expires_at);
CREATE INDEX cache_entries_created_at ON cache_entries (created_at);

-- Tier-2 lexical index: FTS5 external-content table over cache_entries.
--
-- `titles` and `snippets` are INDEX-ONLY columns: they are computed by the
-- triggers from payload_json -> $.results and are not retrievable from
-- cache_fts (external content on a table that lacks those columns reads back
-- NULL, and snippet()/highlight() cannot see them). Anything that needs the
-- text back (W1 search_archive highlighting) must read payload_json from
-- cache_entries or revisit this.
--
-- The scalar subqueries are deliberate: a bare `SELECT new.rowid, ...,
-- group_concat(...) FROM json_each(...)` is an aggregate that emits ZERO rows
-- when $.results is empty (a normal path: the pipeline caches zero-result
-- responses) and NULL rowid values in that shape auto-allocate a phantom FTS
-- rowid the delete trigger can never match, desyncing the index. VALUES keeps
-- the rowid outside any aggregate, so every cache_entries row maps to exactly
-- one cache_fts row.
CREATE VIRTUAL TABLE cache_fts USING fts5 (
    query,
    titles,
    snippets,
    content = 'cache_entries',
    content_rowid = 'rowid'
);

CREATE TRIGGER cache_entries_fts_ai AFTER INSERT ON cache_entries BEGIN
    INSERT INTO cache_fts (rowid, query, titles, snippets)
    VALUES (
        new.rowid,
        new.query,
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.title'), ' ')
               FROM json_each(new.payload_json, '$.results') AS r),
            ''
        ),
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.snippet'), ' ')
               FROM json_each(new.payload_json, '$.results') AS r),
            ''
        )
    );
END;

CREATE TRIGGER cache_entries_fts_ad AFTER DELETE ON cache_entries BEGIN
    INSERT INTO cache_fts (cache_fts, rowid, query, titles, snippets)
    VALUES (
        'delete',
        old.rowid,
        old.query,
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.title'), ' ')
               FROM json_each(old.payload_json, '$.results') AS r),
            ''
        ),
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.snippet'), ' ')
               FROM json_each(old.payload_json, '$.results') AS r),
            ''
        )
    );
END;

CREATE TRIGGER cache_entries_fts_au
    AFTER UPDATE OF query, payload_json ON cache_entries
BEGIN
    INSERT INTO cache_fts (cache_fts, rowid, query, titles, snippets)
    VALUES (
        'delete',
        old.rowid,
        old.query,
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.title'), ' ')
               FROM json_each(old.payload_json, '$.results') AS r),
            ''
        ),
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.snippet'), ' ')
               FROM json_each(old.payload_json, '$.results') AS r),
            ''
        )
    );
    INSERT INTO cache_fts (rowid, query, titles, snippets)
    VALUES (
        new.rowid,
        new.query,
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.title'), ' ')
               FROM json_each(new.payload_json, '$.results') AS r),
            ''
        ),
        coalesce(
            (SELECT group_concat(json_extract(r.value, '$.snippet'), ' ')
               FROM json_each(new.payload_json, '$.results') AS r),
            ''
        )
    );
END;

-- Unconditional request log: every search, cache hit or not. Single writer:
-- Store::log_search (called by Pipeline::respond).
CREATE TABLE search_log (
    id           INTEGER PRIMARY KEY,
    ts           INTEGER NOT NULL,
    query_hash   TEXT NOT NULL,
    query        TEXT NOT NULL,
    client       TEXT NOT NULL,
    source       TEXT NOT NULL,
    tier         INTEGER,
    latency_ms   INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    engines_json TEXT NOT NULL,
    deadline_hit INTEGER NOT NULL
);
CREATE INDEX search_log_ts ON search_log (ts);
CREATE INDEX search_log_query_hash ON search_log (query_hash);

-- Click-through beacons. Single writer: Store::record_click.
CREATE TABLE clicks (
    id         INTEGER PRIMARY KEY,
    ts         INTEGER NOT NULL,
    query_hash TEXT,
    url        TEXT NOT NULL,
    title      TEXT NOT NULL,
    position   INTEGER NOT NULL,
    client     TEXT NOT NULL
);
CREATE INDEX clicks_ts ON clicks (ts);
CREATE INDEX clicks_query_hash ON clicks (query_hash);

-- Scheduler state persisted across restarts. Single writer:
-- Store::put_health.
CREATE TABLE engine_health (
    engine_id     TEXT PRIMARY KEY,
    ewma_ms       REAL NOT NULL,
    failures      INTEGER NOT NULL,
    breaker_state TEXT NOT NULL,
    breaker_until INTEGER,
    last_ok_at    INTEGER,
    last_error    TEXT
);

-- Attributed admin/AI/MCP events. Single writer: Store::audit.
CREATE TABLE audit (
    id           INTEGER PRIMARY KEY,
    ts           INTEGER NOT NULL,
    actor        TEXT NOT NULL,
    action       TEXT NOT NULL,
    target       TEXT NOT NULL,
    details_json TEXT NOT NULL,
    request_id   TEXT
);
CREATE INDEX audit_ts ON audit (ts);
