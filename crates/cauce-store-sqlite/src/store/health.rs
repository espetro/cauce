//! `engine_health`: read-all for the breaker/health panel and upsert from
//! the pipeline's `HealthTracker`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{EngineHealthRow, StoreError};
use rusqlite::params;

use crate::rows;

use super::*;

impl SqliteStore {
    pub(super) async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
        self.with_reader(|conn| {
            conn.prepare(
                "SELECT engine_id, ewma_ms, failures, breaker_state,
                        breaker_until, last_ok_at, last_error
                   FROM engine_health ORDER BY engine_id",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| rows::health(r).map_err(rows::as_sql))
                    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    pub(super) async fn put_health(&self, row: &EngineHealthRow) -> Result<(), StoreError> {
        let row = row.clone();
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO engine_health
                    (engine_id, ewma_ms, failures, breaker_state,
                     breaker_until, last_ok_at, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(engine_id) DO UPDATE SET
                    ewma_ms = excluded.ewma_ms,
                    failures = excluded.failures,
                    breaker_state = excluded.breaker_state,
                    breaker_until = excluded.breaker_until,
                    last_ok_at = excluded.last_ok_at,
                    last_error = excluded.last_error",
                params![
                    row.engine.as_str(),
                    row.ewma_ms,
                    i64::from(row.failures),
                    rows::breaker_str(row.breaker),
                    row.breaker_until.map(|t| rows::to_ms(&t)),
                    row.last_ok_at.map(|t| rows::to_ms(&t)),
                    row.last_error,
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }
}
