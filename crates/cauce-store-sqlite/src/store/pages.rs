//! `pages`: the W5-01 page archive — `put_page` upserts a fetched page
//! (`pages_fts` follows via its triggers; `INSERT OR REPLACE` resolves the
//! PK conflict as delete + insert, which is what fires both), `get_page`
//! reads one row by stored URL.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{PageRow, StoreError};
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
}
