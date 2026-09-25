-- W5-01 page archive: `pages` (fetched, readability-extracted pages) plus
-- `pages_fts`, an FTS5 external-content index over title + markdown.
--
-- `pages.url` is the primary key and the table keeps its implicit rowid, so
-- external-content FTS rows map by `content_rowid = 'rowid'` exactly like
-- `cache_fts`. `put_page` writes with `INSERT ... ON CONFLICT DO UPDATE`
-- (the `cache_entries` upsert's pattern): the conflict fires
-- `pages_fts_au`, which deletes the stale index row and re-inserts it.
-- `INSERT OR REPLACE` would be wrong here — REPLACE's implicit delete does
-- not fire `AFTER DELETE` triggers, leaving ghost terms in the index.
--
-- Triggers use VALUES lists, never `SELECT ... FROM` aggregates: an empty
-- aggregate emits zero rows and would leave the FTS row stale (the
-- cache_fts trigger comment explains the desync in detail).
CREATE TABLE pages (
    url               TEXT PRIMARY KEY,
    fetched_at        INTEGER NOT NULL,
    title             TEXT NOT NULL,
    markdown          TEXT NOT NULL,
    byte_len          INTEGER NOT NULL,
    source_query_hash TEXT
);

CREATE VIRTUAL TABLE pages_fts USING fts5 (
    title,
    markdown,
    content = 'pages',
    content_rowid = 'rowid'
);

CREATE TRIGGER pages_fts_ai AFTER INSERT ON pages BEGIN
    INSERT INTO pages_fts (rowid, title, markdown)
    VALUES (new.rowid, new.title, new.markdown);
END;

CREATE TRIGGER pages_fts_ad AFTER DELETE ON pages BEGIN
    INSERT INTO pages_fts (pages_fts, rowid, title, markdown)
    VALUES ('delete', old.rowid, old.title, old.markdown);
END;

CREATE TRIGGER pages_fts_au
    AFTER UPDATE OF title, markdown ON pages
BEGIN
    INSERT INTO pages_fts (pages_fts, rowid, title, markdown)
    VALUES ('delete', old.rowid, old.title, old.markdown);
    INSERT INTO pages_fts (rowid, title, markdown)
    VALUES (new.rowid, new.title, new.markdown);
END;
