//! Audit trail (`audit`, `list_audit`, `audit_facets`) conformance checks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::Utc;
use uuid::Uuid;

use crate::store::{AuditFilter, AuditRow, Store};

/// `audit` appends; `list_audit` returns newest-first with `since`, `actor`,
/// `action` and `limit` filters.
pub async fn audit_trail(store: &impl Store) {
    let base = Utc::now();
    let request_id = Uuid::now_v7();

    store
        .audit(AuditRow {
            id: None,
            ts: base - chrono::Duration::seconds(1),
            actor: "api".to_string(),
            action: "cache.delete".to_string(),
            target: "conformance-audit-target".to_string(),
            details: serde_json::json!({"key": "abc"}),
            request_id: Some(request_id),
        })
        .await
        .expect("audit api");
    store
        .audit(AuditRow {
            id: None,
            ts: base,
            actor: "cli".to_string(),
            action: "config.put".to_string(),
            target: "conformance-audit-config".to_string(),
            details: serde_json::Value::Null,
            request_id: None,
        })
        .await
        .expect("audit cli");

    let all = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: None,
            limit: 50,
        })
        .await
        .expect("list_audit");
    let p_api = all
        .iter()
        .position(|r| r.target == "conformance-audit-target")
        .expect("api row listed");
    let p_cli = all
        .iter()
        .position(|r| r.target == "conformance-audit-config")
        .expect("cli row listed");
    assert!(p_cli < p_api, "list_audit is newest-first");
    assert_eq!(all[p_api].request_id, Some(request_id));
    assert_eq!(all[p_api].details, serde_json::json!({"key": "abc"}));

    let by_actor = store
        .list_audit(&AuditFilter {
            since: None,
            actor: Some("cli".to_string()),
            action: None,
            limit: 50,
        })
        .await
        .expect("actor filter");
    assert!(by_actor.iter().all(|r| r.actor == "cli"));
    assert!(
        by_actor
            .iter()
            .any(|r| r.target == "conformance-audit-config")
    );

    let by_action = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: Some("cache.delete".to_string()),
            limit: 50,
        })
        .await
        .expect("action filter");
    assert!(by_action.iter().all(|r| r.action == "cache.delete"));
    assert!(
        by_action
            .iter()
            .any(|r| r.target == "conformance-audit-target")
    );

    let recent = store
        .list_audit(&AuditFilter {
            since: Some(base - chrono::Duration::milliseconds(500)),
            actor: None,
            action: None,
            limit: 50,
        })
        .await
        .expect("since filter");
    assert!(
        recent
            .iter()
            .all(|r| r.ts >= base - chrono::Duration::milliseconds(500))
    );

    let one = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: None,
            limit: 1,
        })
        .await
        .expect("limited audit");
    assert_eq!(one.len(), 1);

    let facets = store.audit_facets().await.expect("audit_facets");
    assert!(facets.actors.contains(&"api".to_string()));
    assert!(facets.actors.contains(&"cli".to_string()));
    assert!(facets.actions.contains(&"cache.delete".to_string()));
    assert!(facets.actions.contains(&"config.put".to_string()));
}
