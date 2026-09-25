//! `answers`: the W4-02 AI answer cache — read honoring TTL, upsert on the
//! caching rule (the caller applies it; the store writes what it is given).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use cauce_core::{AnswerKey, AnswerRow, CachedAnswer, StoreError};
use rusqlite::{OptionalExtension, params};

use crate::rows;

use super::*;

impl SqliteStore {
    /// Fresh `answers` row: rows past `expires_at` never surface here. A
    /// read-only lookup (no hit counter on this table), so it goes to the
    /// reader pool.
    pub(super) async fn get_answer(
        &self,
        key: &AnswerKey,
    ) -> Result<Option<CachedAnswer>, StoreError> {
        let key = key.clone();
        self.with_reader(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT {} FROM answers WHERE key = ?1 AND expires_at > ?2",
                    rows::ANSWER_COLS
                ),
                params![key.as_str(), rows::now_ms()],
                |r| rows::answer(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    /// Upsert. `payload_json`/`sources_json` are the replayable `done`
    /// fields and the cited sources; `query` stays the normalized form the
    /// key hashes.
    pub(super) async fn put_answer(
        &self,
        key: &AnswerKey,
        row: &AnswerRow,
        ttl: Duration,
    ) -> Result<(), StoreError> {
        let key = key.clone();
        let row = row.clone();
        self.with_writer(move |conn| {
            let now = rows::now_ms();
            let expires = now
                .saturating_add(ttl.as_millis().min(i64::MAX as u128) as i64)
                .min(max_ts_ms());
            conn.execute(
                "INSERT INTO answers
                    (key, query, model, payload_json, sources_json, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(key) DO UPDATE SET
                    query = excluded.query,
                    model = excluded.model,
                    payload_json = excluded.payload_json,
                    sources_json = excluded.sources_json,
                    created_at = excluded.created_at,
                    expires_at = excluded.expires_at",
                params![
                    key.as_str(),
                    row.query,
                    row.model,
                    serde_json::to_string(&row.payload)?,
                    serde_json::to_string(&row.sources)?,
                    now,
                    expires,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }
}
