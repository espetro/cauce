//! Acceptance tests for the observability foundation (W0-05).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use super::tail::{Tail, TailFilter};
use super::trace::{LogRecord, log_files, render_trace, trace_request};
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

/// `logs_dir`/`data_dir` delegate to `cauce_core::config::Dirs`: a host with
/// only `XDG_DATA_HOME` set (no `CAUCE_DATA_DIR`) must resolve to
/// `$XDG_DATA_HOME/cauce/logs`, the same place `serve` writes them.
#[test]
fn dirs_honour_xdg_data_home() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();

    let prev_cauce = std::env::var_os("CAUCE_DATA_DIR");
    let prev_xdg = std::env::var_os("XDG_DATA_HOME");
    // SAFETY: serialized by ENV_LOCK; nothing else in this test binary
    // touches these variables. nextest also isolates per process.
    unsafe {
        std::env::remove_var("CAUCE_DATA_DIR");
        std::env::set_var("XDG_DATA_HOME", tmp.path());
    }
    let (logs, data) = (super::logs_dir(), super::data_dir());
    // SAFETY: same as above; restores the captured values.
    unsafe {
        match prev_cauce {
            Some(v) => std::env::set_var("CAUCE_DATA_DIR", v),
            None => std::env::remove_var("CAUCE_DATA_DIR"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    assert_eq!(data, tmp.path().join("cauce"));
    assert_eq!(logs, tmp.path().join("cauce").join("logs"));
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
/// contract and the `cauce trace` rendering.
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

/// The env-controlled filter (`CAUCE_LOG`/`RUST_LOG`) scopes stderr/OTLP only:
/// the JSONL layer keeps its own `info` floor, so `filter = "warn"` must
/// still record info-level span opens/closes and events — otherwise
/// `cauce trace` silently goes empty.
#[test]
fn jsonl_layer_ignores_env_filter_floor() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(dir.path());
    config.filter = "warn".to_string();
    let (dispatch, guard) = build(&config).unwrap();
    let request_id = RequestId::new();

    tracing::dispatcher::with_default(&dispatch, || {
        let request = request_span(request_id);
        let _req = request.enter();
        {
            let engine = tracing::info_span!("engine", engine = "ddgs");
            let _e = engine.enter();
            tracing::info!(results = 3u32, "engine done");
            tracing::warn!("slow upstream");
        }
    });
    drop(guard);

    let files = log_files(&config.logs_dir).unwrap();
    assert_eq!(files.len(), 1, "expected one daily log file");
    let content = std::fs::read_to_string(&files[0]).unwrap();
    let kinds: Vec<String> = content
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert!(
        kinds.iter().any(|k| k == "span_open") && kinds.iter().any(|k| k == "span_close"),
        "info-level spans must reach JSONL under a warn filter: {kinds:?}"
    );
    let records = trace_request(&config.logs_dir, &request_id.to_string()).unwrap();
    let out = render_trace(&request_id.to_string(), &records);
    assert!(
        out.contains("ddgs"),
        "info-level engine span missing from trace under warn filter:\n{out}"
    );
}

/// `audit` emits the JSONL event and forwards to `Store::audit`.
#[test]
fn audit_writes_event_and_row() {
    use std::sync::Mutex;

    use cauce_core::{
        AnswerKey, AnswerRow, AuditFilter, AuditRow, CacheKey, CacheState, CachedAnswer,
        CachedSearch, ClickRow, DeleteSearchLog, EngineHealthRow, HistoryFilter, HistoryItem,
        HistoryStats, SearchLogRow, SearchResponse, StatsSnapshot, Store, StoreError,
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
        async fn evict_expired(&self, _: Duration) -> Result<u64, StoreError> {
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
        async fn get_answer(&self, _: &AnswerKey) -> Result<Option<CachedAnswer>, StoreError> {
            unimplemented!()
        }
        async fn put_answer(
            &self,
            _: &AnswerKey,
            _: &AnswerRow,
            _: Duration,
        ) -> Result<(), StoreError> {
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
        async fn cache_states(&self, _: &[CacheKey]) -> Result<Vec<CacheState>, StoreError> {
            unimplemented!()
        }
        async fn search_hashes(&self, _: &[CacheKey]) -> Result<Vec<CacheKey>, StoreError> {
            unimplemented!()
        }
        async fn history_stats(&self, _: &HistoryFilter) -> Result<HistoryStats, StoreError> {
            unimplemented!()
        }
        async fn suggest(&self, _: &str, _: u32) -> Result<Vec<String>, StoreError> {
            unimplemented!()
        }
        async fn delete_search_log(&self, _: i64) -> Result<Option<DeleteSearchLog>, StoreError> {
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

/// Render fixture JSONL lines through `Tail` (no colour) and collect output
/// plus the open-span flush, the same sequence `cauce tail` runs.
fn tail_render(lines: &[&str], filter: TailFilter) -> Vec<String> {
    let mut tail = Tail::new(filter, false);
    let mut out: Vec<String> = lines
        .iter()
        .filter_map(|l| serde_json::from_str::<LogRecord>(l).ok())
        .filter_map(|r| tail.push(&r))
        .collect();
    out.extend(tail.finish());
    out
}

const REQ_A: &str = "019f3c2a-0000-7000-8000-00000000000a";
// Different 8-char prefix so the `--request` prefix filter can tell A from B.
const REQ_B: &str = "02bb4d3c-0000-7000-8000-00000000000b";

/// W2-09 acceptance: `cauce tail` renders a fixture JSONL, one line per event,
/// and collapses the engine `span_open`/`span_close` pair into
/// `engine=bing 640ms ok`.
#[test]
fn tail_collapses_engine_span_line() {
    let fixture = [
        // pipeline.search opens.
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.000Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":1,"name":"pipeline.search","parent":null,"fields":{{"request_id":"{REQ_A}","query":"tanstack router","client":"api"}}}},"spans":["pipeline.search"]}}"#
        ),
        // engine span opens (declared fields only, like the writer emits).
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.010Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":2,"name":"engine","parent":1,"fields":{{"request_id":"{REQ_A}","engine":"bing","tier":2}}}},"spans":["pipeline.search","engine"]}}"#
        ),
        // an event inside the engine span.
        format!(
            r#"{{"v":1,"kind":"event","ts":"2026-10-28T12:00:00.300Z","level":"WARN","target":"cauce_core::http","request_id":"{REQ_A}","span":{{"id":2,"name":"engine"}},"spans":["pipeline.search","engine"],"fields":{{"message":"slow upstream"}}}}"#
        ),
        // engine span closes with accumulated fields + busy_ms.
        format!(
            r#"{{"v":1,"kind":"span_close","ts":"2026-10-28T12:00:00.640Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":2,"name":"engine","parent":1,"fields":{{"request_id":"{REQ_A}","engine":"bing","status":"ok"}}}},"spans":["pipeline.search","engine"],"busy_ms":640.0}}"#
        ),
        // request-level event after the fan-out.
        format!(
            r#"{{"v":1,"kind":"event","ts":"2026-10-28T12:00:00.700Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":1,"name":"pipeline.search"}},"spans":["pipeline.search"],"fields":{{"message":"merged results","raw":5,"merged":5}}}}"#
        ),
    ];

    let out = tail_render(
        &fixture.iter().map(String::as_str).collect::<Vec<_>>(),
        TailFilter::default(),
    );

    // The collapsed engine line: fields inline, duration, status.
    let engine = out
        .iter()
        .find(|l| l.contains("engine=bing"))
        .expect("engine span line missing");
    assert!(
        engine.contains("engine=bing 640ms ok"),
        "collapsed engine line: {engine}"
    );
    // One line per event: the warn and the merged-results event are there,
    // and the two span_opens produced no lines of their own.
    assert!(
        out.iter()
            .any(|l| l.contains("WARN") && l.contains("slow upstream")),
        "warn event missing:\n{}",
        out.join("\n")
    );
    assert!(
        out.iter()
            .any(|l| l.contains("merged results merged=5 raw=5")),
        "event fields missing:\n{}",
        out.join("\n")
    );
    // The still-open pipeline.search span flushes as `open` at EOF.
    assert!(
        out.iter()
            .any(|l| l.contains("query=tanstack router") && l.ends_with("open")),
        "open span flush missing:\n{}",
        out.join("\n")
    );
    assert_eq!(
        out.len(),
        4,
        "expected 4 rendered lines:\n{}",
        out.join("\n")
    );
}

/// `--request`, `--level` and `--engine` filters, including the WARN bump a
/// failed engine span gets so `--level warn` still shows it.
#[test]
fn tail_filters_request_level_engine() {
    let fixture = [
        // Request A: engine bing fails.
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.000Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":1,"name":"engine","parent":null,"fields":{{"request_id":"{REQ_A}","engine":"bing"}}}},"spans":["engine"]}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"event","ts":"2026-10-28T12:00:00.010Z","level":"WARN","target":"cauce_core::http","request_id":"{REQ_A}","span":{{"id":1,"name":"engine"}},"spans":["engine"],"fields":{{"message":"upstream 429"}}}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"span_close","ts":"2026-10-28T12:00:00.012Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_A}","span":{{"id":1,"name":"engine","parent":null,"fields":{{"request_id":"{REQ_A}","engine":"bing","status":"error"}}}},"spans":["engine"],"busy_ms":12.34}}"#
        ),
        // Request B: engine ddgs ok.
        format!(
            r#"{{"v":1,"kind":"span_open","ts":"2026-10-28T12:00:00.020Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_B}","span":{{"id":3,"name":"engine","parent":null,"fields":{{"request_id":"{REQ_B}","engine":"ddgs"}}}},"spans":["engine"]}}"#
        ),
        format!(
            r#"{{"v":1,"kind":"span_close","ts":"2026-10-28T12:00:00.050Z","level":"INFO","target":"cauce_core::pipeline","request_id":"{REQ_B}","span":{{"id":3,"name":"engine","parent":null,"fields":{{"request_id":"{REQ_B}","engine":"ddgs","status":"ok","results":7}}}},"spans":["engine"],"busy_ms":30.0}}"#
        ),
        // A record outside any request.
        r#"{"v":1,"kind":"event","ts":"2026-10-28T12:00:00.060Z","level":"INFO","target":"cauce_server","request_id":null,"span":null,"spans":[],"fields":{"message":"listening"}}"#.to_string(),
    ];
    let lines: Vec<&str> = fixture.iter().map(String::as_str).collect();

    // --request filters to one request id (prefix match).
    let out = tail_render(
        &lines,
        TailFilter {
            request: Some(REQ_A[..8].to_string()),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 2, "request filter:\n{}", out.join("\n"));
    assert!(out.iter().all(|l| l.contains(&REQ_A[..8])));

    // --level warn hides the ok span and info events, keeps the warn event
    // and the failed span (bumped to WARN).
    let out = tail_render(
        &lines,
        TailFilter {
            level: Some("warn".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 2, "level filter:\n{}", out.join("\n"));
    assert!(out.iter().any(|l| l.contains("upstream 429")));
    let failed = out
        .iter()
        .find(|l| l.contains("engine=bing"))
        .expect("failed span should pass --level warn");
    assert!(
        failed.contains("12.3ms error"),
        "failed span line: {failed}"
    );

    // --engine keeps the engine's span and the events inside it.
    let out = tail_render(
        &lines,
        TailFilter {
            engine: Some("bing".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 2, "engine filter:\n{}", out.join("\n"));
    assert!(
        out.iter()
            .all(|l| l.contains("bing") || l.contains("upstream 429"))
    );
}
