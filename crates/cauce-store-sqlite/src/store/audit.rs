//! `audit`: append, filtered list and the distinct actor/action facets.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{AuditFacets, AuditFilter, AuditRow, StoreError};
use rusqlite::params;

use crate::rows;

use super::*;

impl SqliteStore {
    pub(super) async fn audit(&self, row: AuditRow) -> Result<(), StoreError> {
        self.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO audit (ts, actor, action, target, details_json, request_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rows::to_ms(&row.ts),
                    row.actor,
                    row.action,
                    row.target,
                    serde_json::to_string(&row.details)?,
                    row.request_id.map(|u| u.to_string()),
                ],
            )
            .map_err(sql_err)?;
            Ok(())
        })
        .await
    }

    pub(super) async fn list_audit(
        &self,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditRow>, StoreError> {
        let since = filter.since.map(|t| rows::to_ms(&t));
        let actor = filter.actor.clone();
        let action = filter.action.clone();
        let limit = i64::from(filter.limit);
        self.with_reader(move |conn| {
            conn.prepare(
                "SELECT id, ts, actor, action, target, details_json, request_id
                   FROM audit
                  WHERE (?1 IS NULL OR ts >= ?1)
                    AND (?2 IS NULL OR actor = ?2)
                    AND (?3 IS NULL OR action = ?3)
                  ORDER BY ts DESC, id DESC
                  LIMIT ?4",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![since, actor, action, limit], |r| {
                    rows::audit(r).map_err(rows::as_sql)
                })
                .and_then(|m| m.collect::<Result<Vec<_>, _>>())
            })
            .map_err(sql_err)
        })
        .await
    }

    pub(super) async fn audit_facets(&self) -> Result<AuditFacets, StoreError> {
        self.with_reader(|conn| {
            let actors = conn
                .prepare("SELECT DISTINCT actor FROM audit ORDER BY actor")
                .and_then(|mut s| {
                    s.query_map([], |r| r.get::<_, String>(0))
                        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
                })
                .map_err(sql_err)?;
            let actions = conn
                .prepare("SELECT DISTINCT action FROM audit ORDER BY action")
                .and_then(|mut s| {
                    s.query_map([], |r| r.get::<_, String>(0))
                        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
                })
                .map_err(sql_err)?;
            Ok(AuditFacets { actors, actions })
        })
        .await
    }
}
