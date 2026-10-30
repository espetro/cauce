//! `answer_thread`: the UX-01 server-authoritative conversations —
//! owner-scoped upsert, newest-first listing, scoped read and delete.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{AnswerThreadRow, StoreError};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::rows;

use super::*;

impl SqliteStore {
    /// Insert or replace one conversation row, keyed by the
    /// client-minted thread id.
    pub(super) async fn upsert_thread(&self, row: AnswerThreadRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO answer_thread (id, owner, title, created_at, updated_at, turns_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    owner = excluded.owner,
                    title = excluded.title,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    turns_json = excluded.turns_json",
                params![
                    row.id.to_string(),
                    row.owner,
                    row.title,
                    rows::to_ms(&row.created_at),
                    rows::to_ms(&row.updated_at),
                    serde_json::to_string(&row.turns)?,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    /// The owner's threads, `updated_at` newest-first, capped at `limit`.
    pub(super) async fn list_threads(
        &self,
        owner: String,
        limit: u32,
    ) -> Result<Vec<AnswerThreadRow>, StoreError> {
        self.with_reader(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {} FROM answer_thread WHERE owner = ?1
                     ORDER BY updated_at DESC LIMIT ?2",
                    rows::ANSWER_THREAD_COLS
                ))
                .map_err(sql_err)?;
            let listed = stmt
                .query_map(params![owner, limit], |r| {
                    rows::answer_thread(r).map_err(rows::as_sql)
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;
            Ok(listed)
        })
        .await
    }

    /// One thread by id under `owner`; a foreign owner reads `None` —
    /// the 404 rule, never a leak.
    pub(super) async fn get_thread(
        &self,
        id: String,
        owner: String,
    ) -> Result<Option<AnswerThreadRow>, StoreError> {
        self.with_reader(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT {} FROM answer_thread WHERE id = ?1 AND owner = ?2",
                    rows::ANSWER_THREAD_COLS
                ),
                params![id, owner],
                |r| rows::answer_thread(r).map_err(rows::as_sql),
            )
            .optional()
            .map_err(sql_err)
        })
        .await
    }

    /// Remove one thread; false when no row has that id under `owner`.
    pub(super) async fn delete_thread(
        &self,
        id: String,
        owner: String,
    ) -> Result<bool, StoreError> {
        // Validate the shape once here so a malformed id never silently
        // deletes nothing: the handlers parse the UUID first, this is
        // the store-side backstop.
        Uuid::parse_str(&id).map_err(|e| StoreError::Corrupt(format!("thread id {id:?}: {e}")))?;
        self.with_writer(move |conn| {
            conn.execute(
                "DELETE FROM answer_thread WHERE id = ?1 AND owner = ?2",
                params![id, owner],
            )
            .map(|n| n > 0)
            .map_err(sql_err)
        })
        .await
    }
}
