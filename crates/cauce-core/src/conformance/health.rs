//! Engine health (`put_health`, `health`) conformance checks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::Utc;

use crate::engine::EngineId;
use crate::store::{BreakerState, EngineHealthRow, Store};

/// `put_health` upserts by engine id; `health` returns all rows.
pub async fn engine_health(store: &impl Store) {
    let engine = EngineId::from("conf-engine-health");
    let row = EngineHealthRow {
        engine: engine.clone(),
        ewma_ms: 123.5,
        failures: 2,
        breaker: BreakerState::Open,
        breaker_until: Some(Utc::now() + chrono::Duration::minutes(5)),
        last_ok_at: Some(Utc::now() - chrono::Duration::minutes(1)),
        last_error: Some("timeout".to_string()),
    };
    store.put_health(&row).await.expect("put_health");

    let all = store.health().await.expect("health");
    let got = all
        .iter()
        .find(|r| r.engine == engine)
        .expect("row persisted");
    assert_eq!(got.ewma_ms, row.ewma_ms);
    assert_eq!(got.failures, 2);
    assert_eq!(got.breaker, BreakerState::Open);
    assert!(got.breaker_until.is_some());
    assert_eq!(got.last_error.as_deref(), Some("timeout"));

    // Upsert: same engine, fresh state, still exactly one row.
    let reset = EngineHealthRow {
        engine: engine.clone(),
        ewma_ms: 0.0,
        failures: 0,
        breaker: BreakerState::Closed,
        breaker_until: None,
        last_ok_at: Some(Utc::now()),
        last_error: None,
    };
    store.put_health(&reset).await.expect("put_health reset");
    let all = store.health().await.expect("health");
    assert_eq!(all.iter().filter(|r| r.engine == engine).count(), 1);
    assert_eq!(
        all.iter().find(|r| r.engine == engine).unwrap().breaker,
        BreakerState::Closed
    );
}
