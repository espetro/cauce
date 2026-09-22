//! `GET /api/engines` and `POST /api/engines/{id}/reset` (W1-06): the
//! health surface over the pipeline's live tracker, with reset audited.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use oxe_core::config::Config;
use oxe_core::{SearchPipeline, StoreTuning};
use oxe_engines::{Replay, ReplayOpts};
use oxe_server::{AppState, build_router};
use oxe_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

fn state_with(replay: ReplayOpts) -> (Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(replay))],
    ));
    let state = AppState::new(pipeline, store, Config::default());
    (build_router(state), tmp)
}

async fn call(router: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Healthy engine: listed as `closed`; reset is a 200 no-op state-wise and
/// is audited; unknown ids 404.
#[tokio::test]
async fn engines_list_and_reset() {
    let (router, _tmp) = state_with(ReplayOpts::default());

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body.as_array().expect("engine list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["engine"], "replay");
    assert_eq!(rows[0]["breaker"], "closed");
    assert_eq!(rows[0]["failures"], 0);

    let (status, body) = call(&router, "POST", "/api/engines/replay/reset").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["engine"], "replay");
    assert_eq!(body["breaker"], "closed");

    let (status, body) = call(&router, "GET", "/api/audit").await;
    assert_eq!(status, StatusCode::OK);
    let audit = body.as_array().unwrap();
    assert!(
        audit
            .iter()
            .any(|r| r["action"] == "engine.reset" && r["target"] == "replay"),
        "engine.reset audit row missing: {body}"
    );

    let (status, body) = call(&router, "POST", "/api/engines/nope/reset").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}

/// A `blocked` replay opens the breaker; `GET /api/engines` reports it,
/// the next search is breaker-skipped (503), and reset re-admits the
/// engine (which fails again and re-opens).
#[tokio::test]
async fn blocked_engine_reports_open_and_reset_readmits() {
    let (router, _tmp) = state_with(ReplayOpts {
        blocked: true,
        ..ReplayOpts::default()
    });

    let (status, body) = call(&router, "GET", "/api/search?q=br1").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK);
    let row = &body[0];
    assert_eq!(row["breaker"], "open", "{row}");
    assert_eq!(row["failures"], 1);
    assert!(row["breaker_until"].is_string());
    assert!(row["last_error"].is_string());

    // Open engine is skipped, not called: 503 `breaker_open`.
    let (status, body) = call(&router, "GET", "/api/search?q=br2").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "breaker_open");

    // Reset closes the breaker; the next search reaches the engine again
    // (it fails once more and re-opens).
    let (status, body) = call(&router, "POST", "/api/engines/replay/reset").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["breaker"], "closed");
    assert_eq!(body["failures"], 0);
    assert_eq!(body["breaker_until"], Value::Null);

    let (status, body) = call(&router, "GET", "/api/search?q=br3").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    let (status, body) = call(&router, "GET", "/api/engines").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["breaker"], json!("open"), "{body}");
}
