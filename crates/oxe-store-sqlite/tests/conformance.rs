//! Conformance suite (`oxe-core::conformance`) run against a temp-file
//! `SqliteStore`, plus the eviction-task loop.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use oxe_core::StoreTuning;
use oxe_store_sqlite::{SqliteStore, spawn_eviction_task_every};
use tempfile::TempDir;

fn open() -> (SqliteStore, TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store =
        SqliteStore::open(dir.path().join("oxe.db"), StoreTuning::default()).expect("open store");
    (store, dir)
}

#[tokio::test]
async fn cache_exact_roundtrip() {
    let (store, _dir) = open();
    oxe_core::conformance::cache_exact_roundtrip(&store).await;
}

#[tokio::test]
async fn cache_expiry_and_eviction() {
    let (store, _dir) = open();
    oxe_core::conformance::cache_expiry_and_eviction(&store).await;
}

#[tokio::test]
async fn lexical_search() {
    let (store, _dir) = open();
    oxe_core::conformance::lexical_search(&store).await;
}

#[tokio::test]
async fn cache_admin() {
    let (store, _dir) = open();
    oxe_core::conformance::cache_admin(&store).await;
}

#[tokio::test]
async fn log_clicks_history() {
    let (store, _dir) = open();
    oxe_core::conformance::log_clicks_history(&store).await;
}

#[tokio::test]
async fn stats_aggregates() {
    let (store, _dir) = open();
    oxe_core::conformance::stats_aggregates(&store).await;
}

#[tokio::test]
async fn engine_health() {
    let (store, _dir) = open();
    oxe_core::conformance::engine_health(&store).await;
}

#[tokio::test]
async fn audit_trail() {
    let (store, _dir) = open();
    oxe_core::conformance::audit_trail(&store).await;
}

#[tokio::test]
async fn run_all_on_one_store() {
    let (store, _dir) = open();
    oxe_core::conformance::run_all(&store).await;
}

#[tokio::test]
async fn eviction_task_removes_expired_rows() {
    let (store, _dir) = open();
    let store = Arc::new(store);

    let req = oxe_core::conformance::request("eviction task test");
    let key = oxe_core::CacheKey::from(&req);
    let resp = oxe_core::conformance::response(
        "eviction task test",
        &[("R", "https://r.example.com/", "r")],
    );
    use oxe_core::Store;
    store.put(&key, &resp, Duration::ZERO).await.unwrap();
    assert!(store.get_cache(&key).await.unwrap().is_some());

    let task = spawn_eviction_task_every(store.clone(), Duration::from_millis(25));

    let mut gone = false;
    for _ in 0..100 {
        if store.get_cache(&key).await.unwrap().is_none() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    task.abort();
    assert!(gone, "eviction task did not remove the expired row");
}
