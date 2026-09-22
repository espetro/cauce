//! Numbered SQL migrations, embedded at build time and applied in order.
//!
//! Each file under `migrations/` is one schema version, applied inside its
//! own transaction and recorded in `schema_version`. New migrations append to
//! `MIGRATIONS`; never edit a file that has shipped.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use oxe_core::StoreError;
use rusqlite::Connection;

/// `(name, sql)` pairs in apply order; index + 1 is the schema version.
const MIGRATIONS: &[(&str, &str)] = &[("0001_init", include_str!("../migrations/0001_init.sql"))];

fn current_version(conn: &Connection) -> Result<u32, StoreError> {
    conn.query_row(
        "SELECT coalesce(max(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )
    .map_err(|e| StoreError::Backend(e.to_string()))
}

/// Create the bookkeeping table if needed, then apply every migration past
/// the recorded version, each in its own transaction.
pub fn apply_pending(conn: &mut Connection) -> Result<(), StoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );",
    )
    .map_err(|e| StoreError::Backend(e.to_string()))?;

    let applied = current_version(conn)?;
    for (idx, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = idx as u32 + 1;
        if version <= applied {
            continue;
        }
        let tx = conn
            .transaction()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        tx.execute_batch(sql)
            .map_err(|e| StoreError::Backend(format!("migration {name}: {e}")))?;
        tx.execute(
            "INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![version, name, chrono::Utc::now().timestamp_millis()],
        )
        .map_err(|e| StoreError::Backend(e.to_string()))?;
        tx.commit()
            .map_err(|e| StoreError::Backend(e.to_string()))?;
    }
    Ok(())
}
