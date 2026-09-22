//! Acceptance tests for the observability foundation (W0-05).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use super::trace::{log_files, render_trace, trace_request};
use super::{ObservabilityConfig, RequestId, build, request_span};

fn test_config(dir: &std::path::Path) -> ObservabilityConfig {
    ObservabilityConfig {
        logs_dir: dir.join("logs"),
        retention_days: 7,
        stderr_pretty: false,
        filter: "debug".to_string(),
        otlp: false,
    }
}

/// `logs_dir`/`data_dir` delegate to `oxe_core::config::Dirs`: a host with
/// only `XDG_DATA_HOME` set (no `OXE_DATA_DIR`) must resolve to
/// `$XDG_DATA_HOME/oxe/logs`, the same place `serve` writes them.
#[test]
fn dirs_honour_xdg_data_home() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();

    let prev_oxe = std::env::var_os("OXE_DATA_DIR");
    let prev_xdg = std::env::var_os("XDG_DATA_HOME");
    // SAFETY: serialized by ENV_LOCK; nothing else in this test binary
    // touches these variables. nextest also isolates per process.
    unsafe {
        std::env::remove_var("OXE_DATA_DIR");
        std::env::set_var("XDG_DATA_HOME", tmp.path());
    }
    let (logs, data) = (super::logs_dir(), super::data_dir());
    // SAFETY: same as above; restores the captured values.
    unsafe {
        match prev_oxe {
            Some(v) => std::env::set_var("OXE_DATA_DIR", v),
            None => std::env::remove_var("OXE_DATA_DIR"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    assert_eq!(data, tmp.path().join("oxe"));
    assert_eq!(logs, tmp.path().join("oxe").join("logs"));
}

/// Minimal `block_on` so tests can drive `Store` futures without a runtime.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll};

    let mut cx = Context::from_waker(std::task::Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// Drive a fake pipeline span tree, flush, then check both the JSONL schema
/// contract and the `oxe trace` rendering.
#[test]
fn trace_replays_engine_spans_in_order_with_durations() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let (dispatch, guard) = build(&config).unwrap();
    let request_id = RequestId::new();

    tracing::dispatcher::with_default(&dispatch, || {
        let request = request_span(request_id);
        let _req = request.enter();
        {
            let cache = tracing::info_span!("cache_lookup", tier = 1u8);
            let _c = cache.enter();
            tracing::info!(hit = false, "cache miss");
        }
        {
            let engine = tracing::info_span!("engine", engine = "ddgs");
            let _e = engine.enter();
            std::thread::sleep(Duration::from_millis(5));
            tracing::info!(results = 10u32, "engine done");
        }
        {
            let engine = tracing::info_span!("engine", engine = "bing");
            let _e = engine.enter();
            std::thread::sleep(Duration::from_millis(5));
            tracing::warn!("slow upstream");
        }
        tracing::error!("synthetic failure marker");
    });
    drop(guard);

    // Every JSONL line parses and carries the contract fields.
    let files = log_files(&config.logs_dir).unwrap();
    assert_eq!(files.len(), 1, "expected one daily log file");
    let content = std::fs::read_to_string(&files[0]).unwrap();
    let mut saw_close = false;
    for line in content.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        for key in ["request_id", "level", "target", "ts"] {
            assert!(value.get(key).is_some(), "line missing {key:?}: {line}");
        }
        assert_eq!(
            value["request_id"].as_str().unwrap(),
            request_id.to_string(),
            "line outside the request span: {line}"
        );
        if value["kind"] == "span_close" {
            saw_close = true;
        }
    }
    assert!(saw_close, "no span_close records written");

    // The trace timeline lists the engine spans in fan-out order with
    // durations, plus the cache decision and the error.
    let records = trace_request(&config.logs_dir, &request_id.to_string()).unwrap();
    assert!(!records.is_empty());
    let out = render_trace(&request_id.to_string(), &records);
    let ddgs = out.find("ddgs").expect("ddgs span missing");
    let bing = out.find("bing").expect("bing span missing");
    assert!(ddgs < bing, "engine spans out of order:\n{out}");
    assert!(out.contains("ms"), "durations missing:\n{out}");
    assert!(out.contains("tier=1"), "cache decision missing:\n{out}");
    assert!(out.contains("ERROR"), "error event missing:\n{out}");
}

/// `audit` emits the JSONL event and forwards to `Store::audit`.
#[test]
fn audit_writes_event_and_row() {
    use std::sync::Mutex;

    use oxe_core::{
        AuditFilter, AuditRow, CacheKey, CachedSearch, ClickRow, EngineHealthRow, HistoryFilter,
        HistoryItem, SearchLogRow, SearchResponse, StatsSnapshot, Store, StoreError,
    };

    struct Spy {
        rows: Mutex<Vec<AuditRow>>,
    }

    #[async_trait::async_trait]
    impl Store for Spy {
        async fn get_exact(&self, _: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
            unimplemented!()
        }
        async fn get_lexical(&self, _: &str, _: u8) -> Result<Vec<CachedSearch>, StoreError> {
            unimplemented!()
        }
        async fn put(
            &self,
            _: &CacheKey,
            _: &SearchResponse,
            _: Duration,
        ) -> Result<(), StoreError> {
            unimplemented!()
        }
        async fn evict_expired(&self) -> Result<u64, StoreError> {
            unimplemented!()
        }
        async fn list_cache(&self, _: u32, _: u32) -> Result<Vec<CachedSearch>, StoreError> {
            unimplemented!()
        }
        async fn get_cache(&self, _: &CacheKey) -> Result<Option<CachedSearch>, StoreError> {
            unimplemented!()
        }
        async fn delete_cache(&self, _: &CacheKey) -> Result<bool, StoreError> {
            unimplemented!()
        }
        async fn clear_cache(&self) -> Result<u64, StoreError> {
            unimplemented!()
        }
        async fn log_search(&self, _: SearchLogRow) -> Result<(), StoreError> {
            unimplemented!()
        }
        async fn record_click(&self, _: ClickRow) -> Result<(), StoreError> {
            unimplemented!()
        }
        async fn list_history(&self, _: &HistoryFilter) -> Result<Vec<HistoryItem>, StoreError> {
            unimplemented!()
        }
        async fn stats(&self, _: u32) -> Result<StatsSnapshot, StoreError> {
            unimplemented!()
        }
        async fn health(&self) -> Result<Vec<EngineHealthRow>, StoreError> {
            unimplemented!()
        }
        async fn put_health(&self, _: &EngineHealthRow) -> Result<(), StoreError> {
            unimplemented!()
        }
        async fn audit(&self, row: AuditRow) -> Result<(), StoreError> {
            self.rows.lock().unwrap().push(row);
            Ok(())
        }
        async fn list_audit(&self, _: &AuditFilter) -> Result<Vec<AuditRow>, StoreError> {
            unimplemented!()
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let (dispatch, guard) = build(&config).unwrap();
    let request_id = RequestId::new();

    let store = Spy {
        rows: Mutex::new(Vec::new()),
    };
    let row = AuditRow {
        id: None,
        ts: chrono::Utc::now(),
        actor: "cli".to_string(),
        action: "cache.delete".to_string(),
        target: "deadbeef".to_string(),
        details: serde_json::json!({}),
        request_id: Some(request_id.as_uuid()),
    };

    let result = tracing::dispatcher::with_default(&dispatch, || {
        let span = request_span(request_id);
        let _r = span.enter();
        block_on(super::audit(&store, row))
    });
    assert!(result.is_ok());
    assert_eq!(store.rows.lock().unwrap().len(), 1);
    drop(guard);

    let files = log_files(&config.logs_dir).unwrap();
    let content = std::fs::read_to_string(&files[0]).unwrap();
    let audit_line = content
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["fields"]["audit"] == true);
    let line = audit_line.expect("audit event missing");
    assert_eq!(line["fields"]["action"], "cache.delete");
    assert_eq!(line["request_id"], request_id.to_string());
}
