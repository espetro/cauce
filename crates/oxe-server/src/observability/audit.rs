//! `audit` helper: one call site per audited action.
//!
//! Emits a JSONL event with `audit = true` (so `oxe trace` and `/audit`
//! tooling can pick it out of the stream) and appends the row to the
//! `audit` table through `Store::audit` (parent plan 6.1: config changes,
//! cache deletes, breaker transitions, spec loads, AI provider calls, MCP
//! tool invocations).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use oxe_core::{AuditRow, Store, StoreError};

/// Emit the audit event and persist the row.
///
/// `store` is `&dyn Store` friendly: pass `store.as_ref()` for
/// `Arc<dyn Store>` holders. The event is emitted before the write so a
/// failed insert still leaves a trace line; a store error is logged and
/// propagated.
pub async fn audit<S: Store + ?Sized>(store: &S, row: AuditRow) -> Result<(), StoreError> {
    match row.request_id {
        Some(request_id) => tracing::info!(
            target: "oxe.audit",
            audit = true,
            actor = %row.actor,
            action = %row.action,
            audit_target = %row.target,
            request_id = %request_id,
            "audit"
        ),
        None => tracing::info!(
            target: "oxe.audit",
            audit = true,
            actor = %row.actor,
            action = %row.action,
            audit_target = %row.target,
            "audit"
        ),
    }
    store.audit(row).await.inspect_err(|e| {
        tracing::error!(
            target: "oxe.audit",
            audit = true,
            error = %e,
            "audit write failed"
        );
    })
}
