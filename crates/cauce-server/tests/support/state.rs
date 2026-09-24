//! Tempdir-backed `AppState` builders: SqliteStore plus the
//! deterministic `replay` engine.

use std::sync::Arc;

use axum::Router;
use cauce_core::config::Config;
use cauce_core::{SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;

/// A tempdir-backed `AppState`: `SqliteStore` plus a single `replay`
/// engine built from `replay`, serving `config`.
#[allow(dead_code)]
pub fn replay_state(replay: ReplayOpts, config: Config) -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(replay))],
    ));
    (AppState::new(pipeline, store, config), tmp)
}

/// `replay_state` with the default engine and config.
#[allow(dead_code)]
pub fn test_state() -> (AppState, tempfile::TempDir) {
    replay_state(ReplayOpts::default(), Config::default())
}

/// `replay_state` with caller-chosen `ReplayOpts` (blocked, latency, ...).
#[allow(dead_code)]
pub fn test_state_with(replay: ReplayOpts) -> (AppState, tempfile::TempDir) {
    replay_state(replay, Config::default())
}

/// `replay_state` with a caller-chosen `Config`.
#[allow(dead_code)]
pub fn test_state_with_config(config: Config) -> (AppState, tempfile::TempDir) {
    replay_state(ReplayOpts::default(), config)
}

/// `test_state` behind the default router: `(router, state, tmp)`.
#[allow(dead_code)]
pub fn app() -> (Router, AppState, tempfile::TempDir) {
    let (state, tmp) = test_state();
    (build_router(state.clone()), state, tmp)
}

/// `app` with caller-chosen `ReplayOpts`.
#[allow(dead_code)]
pub fn app_with(replay: ReplayOpts) -> (Router, AppState, tempfile::TempDir) {
    let (state, tmp) = test_state_with(replay);
    (build_router(state.clone()), state, tmp)
}
