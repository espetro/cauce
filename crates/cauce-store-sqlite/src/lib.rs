//! cauce-store-sqlite: rusqlite implementation of the `cauce_core::Store` trait
//! (bundled SQLite with FTS5).
//!
//! Concurrency model (wave-0 spec): one writer connection behind a
//! `tokio::sync::Mutex` plus a small round-robin read pool; every rusqlite
//! call runs inside `tokio::task::spawn_blocking`. Schema lives in numbered
//! SQL files under `migrations/`, embedded with `include_str!` and tracked in
//! a `schema_version` table.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod evict;
mod migrate;
mod rows;
mod store;

pub use evict::{EVICTION_INTERVAL, spawn_eviction_task, spawn_eviction_task_every};
pub use store::{READ_POOL_SIZE, SqliteStore};
