//! `pages`: the W5-01 page archive — `put_page` upserts a fetched page
//! (`pages_fts` follows via its triggers; `INSERT OR REPLACE` resolves the
//! PK conflict as delete + insert, which is what fires both), `get_page`
//! reads one row by stored URL, `search_pages`/`list_pages` feed the W5-02
//! archive surface and `delete_page` removes a row (the `pages_fts_ad`
//! trigger cleans the index with it).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{PAGE_MARK_CLOSE, PAGE_MARK_OPEN, PageHit, PageRow, StoreError};
use rusqlite::{OptionalExtension, params};
use url::Url;

use crate::rows;

use super::*;

impl SqliteStore {
    /// Upsert one archived page. `ON CONFLICT DO UPDATE` is deliberate
    /// (the `cache_entries` upsert's pattern): the conflict fires
    /// `pages_fts_au`, rebuilding the index row. `INSERT OR REPLACE`'s
    /// implicit delete does not fire `AFTER DELETE` triggers — ghost
    /// terms would stay indexed.
    pub(super) async fn put_page(&self, row: &PageRow) -> Result<(), StoreError> {
        let row = row.clone();
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO pages
                    (url, fetched_at, title, markdown, byte_len, source_query_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(url) DO UPDATE SET
                    fetched_at = excluded.fetched_at,
                    title = excluded.title,
                    markdown = excluded.markdown,
                    byte_len = excluded.byte_len,
                    source_query_hash = excluded.source_query_hash",
                params![
                    row.url.as_str(),
                    rows::to_ms(&row.fetched_at),
                    row.title,
                    row.markdown,
                    row.byte_len as i64,
                    row.source_query_hash.as_ref().map(|k| k.as_str()),
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    /// Read one `pages` row by its stored (normalized) URL. Read-only, so
    /// it goes to the reader pool.
    pub(super) async fn get_page(&self, url: &Url) -> Result<Option<PageRow>, StoreError> {
        let url = url.clone();
        self.with_reader(move |conn| {
            conn.query_row(
                &format!("SELECT {} FROM pages WHERE url = ?1", rows::PAGE_COLS),
                params![url.as_str()],
                |r| rows::page(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    /// FTS5 over title + markdown (the `get_lexical` JOIN pattern on the
    /// external-content index), bm25 rank order. `snippet()` excerpts the
    /// markdown column — external content means the real column text is
    /// readable, the `cache_fts` index-only columns could not do this —
    /// with the `PAGE_MARK_*` match delimiters the callers map to
    /// `<mark>` or strip. `q` goes through `fts_query` escaping first, so
    /// user input is phrases, never operators.
    pub(super) async fn search_pages(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<PageHit>, StoreError> {
        let Some(fts) = fts_query(q) else {
            return Ok(Vec::new());
        };
        self.with_reader(move |conn| {
            conn.prepare(&format!(
                "SELECT pages.url, pages.fetched_at, pages.title,
                        snippet(pages_fts, 1, '{PAGE_MARK_OPEN}', '{PAGE_MARK_CLOSE}', '…', 40) AS snippet,
                        bm25(pages_fts) AS score
                  FROM pages_fts
                  JOIN pages ON pages.rowid = pages_fts.rowid
                  WHERE pages_fts MATCH ?1
                  ORDER BY rank
                  LIMIT ?2"
            ))
            .and_then(|mut stmt| {
                stmt.query_map(params![fts, i64::from(limit)], page_hit)
                    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    /// Newest `pages` rows first. `substr` reads only the excerpt prefix
    /// — `markdown` rows can be 200 KB and the listing needs no body.
    pub(super) async fn list_pages(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<PageHit>, StoreError> {
        self.with_reader(move |conn| {
            conn.prepare(
                "SELECT url, fetched_at, title, substr(markdown, 1, 240) AS snippet,
                        NULL AS score
                  FROM pages
                  ORDER BY fetched_at DESC, rowid DESC
                  LIMIT ?1 OFFSET ?2",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![i64::from(limit), i64::from(offset)], page_hit)
                    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    /// Remove one `pages` row by stored URL; `pages_fts_ad` cleans the
    /// index row (external content → the trigger supplies the delete-row
    /// content form).
    pub(super) async fn delete_page(&self, url: &Url) -> Result<bool, StoreError> {
        let url = url.clone();
        self.with_writer(move |conn| {
            conn.execute("DELETE FROM pages WHERE url = ?1", params![url.as_str()])
                .map(|n| n > 0)
                .map_err(sql_err)
        })
        .await
    }
}

/// Decode one hit row selected as `url, fetched_at, title, snippet, score`
/// (`search_pages`/`list_pages`). `score` reads as `Option` because the
/// listing selects NULL for it.
fn page_hit(r: &rusqlite::Row) -> Result<PageHit, rusqlite::Error> {
    let corrupt = |e: rusqlite::Error| rows::as_sql(StoreError::Corrupt(format!("{e}")));
    Ok(PageHit {
        url: Url::parse(&r.get::<_, String>(0).map_err(&corrupt)?)
            .map_err(|e| rows::as_sql(StoreError::Corrupt(format!("url: {e}"))))?,
        fetched_at: rows::from_ms(r.get(1).map_err(&corrupt)?).map_err(rows::as_sql)?,
        title: r.get(2).map_err(&corrupt)?,
        snippet: r.get(3).map_err(&corrupt)?,
        score: r.get(4).map_err(&corrupt)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `pages_fts` external-content index follows `put_page`: an insert
    /// lands the FTS row, a replace rebuilds it (old terms out, new terms
    /// in) — this is what `INSERT OR REPLACE` buys over `DO UPDATE`.
    #[tokio::test]
    async fn fts_triggers_follow_put_and_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cauce.db");
        let store = SqliteStore::open(&path, StoreTuning::default()).unwrap();
        let url = Url::parse("https://pages.example.com/fts").unwrap();
        let row = PageRow {
            url: url.clone(),
            fetched_at: chrono::Utc::now(),
            title: "Fts page".to_string(),
            markdown: "alpha beta gamma".to_string(),
            byte_len: 42,
            source_query_hash: None,
        };
        store.put_page(&row).await.unwrap();

        // Second connection on the same WAL file: FTS is internal to
        // SQLite, so the assertions read `pages_fts` directly.
        let raw = rusqlite::Connection::open(&path).unwrap();
        let hits = |term: &str| -> i64 {
            raw.query_row(
                "SELECT COUNT(*) FROM pages_fts WHERE pages_fts MATCH ?1",
                params![term],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits("gamma"), 1);

        let mut updated = row;
        updated.markdown = "delta epsilon".to_string();
        store.put_page(&updated).await.unwrap();
        assert_eq!(hits("gamma"), 0, "replaced row must not linger in FTS");
        assert_eq!(hits("epsilon"), 1);
    }

    /// The W5-02 read surface: `search_pages` finds a row by a body phrase
    /// and returns a marked snippet plus a bm25 score, `list_pages` serves
    /// newest-first excerpts, and `delete_page` removes the row AND its
    /// index entry (the `pages_fts_ad` trigger supplies the delete-row
    /// content form external-content FTS needs — a bare shadow delete
    /// would leave ghost terms).
    #[tokio::test]
    async fn search_list_and_delete_pages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cauce.db");
        let store = SqliteStore::open(&path, StoreTuning::default()).unwrap();
        let url = Url::parse("https://pages.example.com/dredging").unwrap();
        let row = PageRow {
            url: url.clone(),
            fetched_at: chrono::Utc::now(),
            title: "Harbour works".to_string(),
            markdown: "the council approved night-time dredging of the inner harbour".to_string(),
            byte_len: 900,
            source_query_hash: None,
        };
        store.put_page(&row).await.unwrap();

        // FTS hit: phrase match on markdown, marked snippet, real score.
        let hits = store.search_pages("night-time dredging", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, url);
        let parts = hits[0].snippet_parts();
        assert!(
            parts.iter().any(|(t, m)| *m && t.contains("dredging")),
            "snippet must mark the matched term: {:?}",
            hits[0].snippet
        );
        assert!(hits[0].plain_snippet().contains("night-time dredging"));
        assert!(hits[0].score.is_some());

        // A query that escapes to nothing is an empty page, not an error.
        assert!(store.search_pages("*?!", 10).await.unwrap().is_empty());

        // Browsing surface: newest-first excerpt, no score, no marks.
        let listed = store.list_pages(10, 0).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].url, url);
        assert!(listed[0].score.is_none());
        assert!(listed[0].snippet.contains("night-time dredging"));

        // Delete removes the row AND cleans the FTS shadow.
        assert!(store.delete_page(&url).await.unwrap());
        assert!(store.get_page(&url).await.unwrap().is_none());
        assert!(
            store.search_pages("dredging", 10).await.unwrap().is_empty(),
            "delete_page must evict the row from pages_fts"
        );
        assert!(!store.delete_page(&url).await.unwrap());
    }
}
