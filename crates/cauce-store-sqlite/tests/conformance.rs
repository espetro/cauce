//! Conformance suite (`cauce-core::conformance`) run against a temp-file
//! `SqliteStore`, plus the eviction-task loop.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use cauce_core::{CacheKey, Store, StoreTuning};
use cauce_store_sqlite::{SqliteStore, spawn_eviction_task_every};
use tempfile::TempDir;

fn open() -> (SqliteStore, TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store =
        SqliteStore::open(dir.path().join("cauce.db"), StoreTuning::default()).expect("open store");
    (store, dir)
}

#[tokio::test]
async fn cache_exact_roundtrip() {
    let (store, _dir) = open();
    cauce_core::conformance::cache_exact_roundtrip(&store).await;
}

#[tokio::test]
async fn cache_expiry_and_eviction() {
    let (store, _dir) = open();
    cauce_core::conformance::cache_expiry_and_eviction(&store).await;
}

#[tokio::test]
async fn lexical_search() {
    let (store, _dir) = open();
    cauce_core::conformance::lexical_search(&store).await;
}

#[tokio::test]
async fn cache_admin() {
    let (store, _dir) = open();
    cauce_core::conformance::cache_admin(&store).await;
}

#[tokio::test]
async fn log_clicks_history() {
    let (store, _dir) = open();
    cauce_core::conformance::log_clicks_history(&store).await;
}

#[tokio::test]
async fn stats_aggregates() {
    let (store, _dir) = open();
    cauce_core::conformance::stats_aggregates(&store).await;
}

#[tokio::test]
async fn engine_health() {
    let (store, _dir) = open();
    cauce_core::conformance::engine_health(&store).await;
}

#[tokio::test]
async fn audit_trail() {
    let (store, _dir) = open();
    cauce_core::conformance::audit_trail(&store).await;
}

#[tokio::test]
async fn run_all_on_one_store() {
    let (store, _dir) = open();
    cauce_core::conformance::run_all(&store).await;
}

#[tokio::test]
async fn eviction_task_removes_expired_rows() {
    let (store, _dir) = open();
    let store = Arc::new(store);

    let req = cauce_core::conformance::request("eviction task test");
    let key = cauce_core::CacheKey::from(&req);
    let resp = cauce_core::conformance::response(
        "eviction task test",
        &[("R", "https://r.example.com/", "r")],
    );
    store.put(&key, &resp, Duration::ZERO).await.unwrap();
    assert!(store.get_cache(&key).await.unwrap().is_some());

    // ZERO grace: the task hard-deletes expired rows (the W3-02 stale-serve
    // window is asserted inside `cache_expiry_and_eviction`).
    let task = spawn_eviction_task_every(store.clone(), Duration::from_millis(25), Duration::ZERO);

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

/// A `SearchResponse` with `results: []` must still produce exactly one
/// `cache_fts` row carrying the `cache_entries` rowid (regression: aggregate
/// trigger SELECTs emitted zero rows and NULL rowid for empty results,
/// auto-allocating a phantom FTS rowid that the delete trigger never matched).
#[tokio::test]
async fn fts_rowid_in_sync_for_empty_results() {
    let (store, dir) = open();
    let path = dir.path().join("cauce.db");

    let empty_key = CacheKey::from(&cauce_core::conformance::request("fts empty"));
    let full_key = CacheKey::from(&cauce_core::conformance::request("fts full"));
    store
        .put(
            &empty_key,
            &cauce_core::conformance::response("fts empty", &[]),
            Duration::from_secs(3600),
        )
        .await
        .unwrap();
    store
        .put(
            &full_key,
            &cauce_core::conformance::response("fts full", &[("T", "https://t.example.com/", "s")]),
            Duration::from_secs(3600),
        )
        .await
        .unwrap();

    // The empty-results row is still findable through its query column.
    let hits = store.get_lexical("fts empty", 10).await.unwrap();
    assert!(hits.iter().any(|c| c.key == empty_key));

    // No FTS rowid without a backing cache_entries row (in either direction).
    // Note: a plain scan of cache_fts fetches its content columns, which do
    // not exist on cache_entries (index-only) — the %_docsize shadow table is
    // the reliable way to list indexed rowids.
    let conn = rusqlite::Connection::open(&path).unwrap();
    let orphans: i64 = conn
        .query_row(
            "SELECT count(*) FROM cache_fts_docsize d
              WHERE NOT EXISTS (SELECT 1 FROM cache_entries e WHERE e.rowid = d.id)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0, "cache_fts rows orphaned from cache_entries");
    let unindexed: i64 = conn
        .query_row(
            "SELECT count(*) FROM cache_entries e
              WHERE NOT EXISTS (SELECT 1 FROM cache_fts_docsize d WHERE d.id = e.rowid)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unindexed, 0, "cache_entries rows missing from cache_fts");

    // Deletes must retract the index rows too.
    store.delete_cache(&empty_key).await.unwrap();
    store.delete_cache(&full_key).await.unwrap();
    let left: i64 = conn
        .query_row("SELECT count(*) FROM cache_fts_docsize", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0, "deleted rows must leave cache_fts");
}

/// Reopening the same file is a no-op migration-wise: `schema_version` stays
/// at the latest migration and data survives.
#[tokio::test]
async fn reopen_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cauce.db");
    let key = CacheKey::from(&cauce_core::conformance::request("reopen"));
    let resp = cauce_core::conformance::response("reopen", &[("R", "https://r.example.com/", "r")]);

    {
        let store = SqliteStore::open(&path, StoreTuning::default()).unwrap();
        store
            .put(&key, &resp, Duration::from_secs(3600))
            .await
            .unwrap();
    }

    let store = SqliteStore::open(&path, StoreTuning::default()).unwrap();
    let got = store
        .get_cache(&key)
        .await
        .unwrap()
        .expect("row survives reopen");
    assert_eq!(got.response, resp);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 2, "reopening must not re-run migrations");
}

/// `:memory:` would give every pooled connection a private database, so it is
/// rejected loudly rather than silently dropping reads.
#[test]
fn memory_database_is_rejected() {
    let err = SqliteStore::open(":memory:", StoreTuning::default())
        .err()
        .expect(":memory: must be rejected");
    assert!(err.to_string().contains(":memory:"));
}
