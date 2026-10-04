//! Report collector tests (#239): `report::collect` against a tempdir
//! state — the v1 section set, profile plumbing, `errors_tail` log
//! parsing and `audit_tail`/`stats` redaction end to end.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;

use axum::http::{StatusCode, header};
use cauce_core::config::{Config, EnvMap};
use cauce_core::report::ReportBundle;
use cauce_core::{
    AuditRow, CacheKey, ClientKind, LogSource, SafeSearch, SearchLogRow, SearchRequest,
};
use cauce_server::{AppState, report::collect};
use chrono::Utc;
use serde_json::{Value, json};

mod support;
use support::*;

/// A tempdir-backed state whose `Config` resolves `data_dir`/`logs_dir`
/// inside `cfg_tmp` (plus an empty `CAUCE_EVAL_RESULTS_DIR`), so the
/// collector reads fixtures the test writes.
async fn fixture(toml_src: &str) -> (AppState, tempfile::TempDir, tempfile::TempDir) {
    let cfg_tmp = tempfile::tempdir().expect("cfg tempdir");
    // Point evals at an empty dir so `eval_latest` is deterministic.
    let evals = cfg_tmp.path().join("evals-results");
    std::fs::create_dir_all(&evals).unwrap();
    unsafe {
        std::env::set_var("CAUCE_EVAL_RESULTS_DIR", &evals);
    }
    let env: EnvMap = BTreeMap::from([
        (
            "CAUCE_DATA_DIR".to_string(),
            cfg_tmp.path().join("data").to_string_lossy().into_owned(),
        ),
        (
            "CAUCE_CONFIG_DIR".to_string(),
            cfg_tmp.path().join("cfg").to_string_lossy().into_owned(),
        ),
    ]);
    let raw: toml::Value = toml::from_str(toml_src).expect("toml");
    let config = Config::from_raw(&raw, &env).expect("config");
    let (state, store_tmp) = test_state_with_config(config);
    (state, store_tmp, cfg_tmp)
}

fn log_row(q: &str, result_count: u32) -> SearchLogRow {
    SearchLogRow {
        id: None,
        ts: Utc::now(),
        query_hash: CacheKey::from(&SearchRequest {
            q: q.to_string(),
            page: 1,
            lang: None,
            time_range: None,
            safesearch: SafeSearch::default(),
            engines: None,
            client: ClientKind::Api,
        }),
        query: q.to_string(),
        query_raw: Some(q.to_string()),
        client: ClientKind::Api,
        source: LogSource::Network,
        tier: None,
        latency_ms: 12,
        result_count,
        engines: vec![],
        deadline_hit: false,
    }
}

/// Warn/error fixture records written as `<data_dir>/logs/…jsonl`.
fn write_logs(cfg_tmp: &tempfile::TempDir) {
    let logs = cfg_tmp.path().join("data/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let lines = [
        // An info event the tail must skip.
        json!({"v": 1, "kind": "event", "ts": "2026-09-28T10:00:00Z",
               "level": "INFO", "target": "cauce::pipeline",
               "fields": {"message": "search ok"}}),
        json!({"v": 1, "kind": "event", "ts": "2026-09-28T10:01:00Z",
               "level": "WARN", "target": "cauce::pipeline",
               "fields": {"message": "engine slow",
                          "query": "my secret query"}}),
        json!({"v": 1, "kind": "span_close", "ts": "2026-09-28T10:02:00Z",
               "level": "ERROR", "target": "cauce::pipeline",
               "span": {"id": 9, "name": "engine_http",
                        "fields": {"query": "my secret query",
                                   "url": "https://u:p@bing.test/search?q=my+secret+query"}},
               "fields": {"message": "engine failed",
                          "error": "dial https://u:p@up.test/x?q=my+secret+query"},
               "busy_ms": 4.0}),
    ];
    let body = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(logs.join("cauce-2026-09-28.jsonl"), body).unwrap();
}

const TOML: &str = r#"
[ai]
api_key = "sk-test-123"
"#;

#[tokio::test]
async fn collect_builds_safe_v1_bundle() {
    let _env = env_lock().await;
    let (state, _store_tmp, cfg_tmp) = fixture(TOML).await;
    write_logs(&cfg_tmp);
    let store = state.store().clone();
    store
        .log_search(log_row("My Secret Query", 10))
        .await
        .unwrap();
    store
        .log_search(log_row("what is my ssn", 0))
        .await
        .unwrap();
    store
        .audit(AuditRow {
            id: None,
            ts: Utc::now(),
            actor: "Jane Doe <jane@example.com>".into(),
            action: "history.delete".into(),
            target: "42".into(),
            details: json!({"query": "leak me", "clicks_removed": 2}),
            request_id: None,
        })
        .await
        .unwrap();

    let bundle = collect(&state, 7, false).await;
    assert_eq!(bundle.v, 1);
    assert_eq!(bundle.profile, cauce_core::RedactionProfile::Safe);

    // The v1 section set is present.
    for key in [
        "cauce",
        "config",
        "stats",
        "engines",
        "audit_tail",
        "errors_tail",
        "storage",
        "eval_latest",
    ] {
        assert!(bundle.sections.contains_key(key), "missing section {key}");
    }

    // `cauce`: process metadata.
    let cauce = &bundle.sections["cauce"];
    assert_eq!(cauce["version"], json!(env!("CARGO_PKG_VERSION")));
    assert!(cauce["uptime_s"].is_u64());
    let features: Vec<&str> = cauce["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::as_str)
        .collect::<Option<_>>()
        .unwrap();
    for f in ["ui", "mcp", "ai", "archive"] {
        assert!(features.contains(&f), "feature {f} missing");
    }
    assert_eq!(cauce["bind"], json!("127.0.0.1:4479"));

    // `config` is the redacted display tree.
    assert_eq!(bundle.sections["config"]["ai"]["api_key"], "<redacted>");

    // `stats` rows carried queries; under `safe` only hashes survive.
    let stats = &bundle.sections["stats"];
    assert_eq!(stats["searches"], json!(2));
    let hash = stats["top_queries"][0]["query_hash"]
        .as_str()
        .expect("query_hash");
    assert_eq!(hash.len(), 64);
    assert!(stats["top_queries"][0].get("query").is_none());
    assert_eq!(stats["zero_result_queries"][0].as_str().unwrap().len(), 64);

    // `engines` sees the live replay engine.
    let engines = bundle.sections["engines"].as_array().unwrap();
    assert!(engines.iter().any(|e| e["engine"] == json!("replay")));

    // `audit_tail`: free-form actor dropped, details query leaf stripped,
    // the rest of the row intact.
    let audit = &bundle.sections["audit_tail"][0];
    assert_eq!(audit["actor"], json!("<redacted>"));
    assert_eq!(audit["action"], json!("history.delete"));
    assert!(audit["details"].get("query").is_none());
    assert_eq!(audit["details"]["clicks_removed"], json!(2));

    // `errors_tail`: info skipped, warn/error kept, query leaves and
    // URLs scrubbed.
    let errors = bundle.sections["errors_tail"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["level"], json!("ERROR"));
    assert_eq!(
        errors[0]["span"]["fields"]["url"],
        json!("https://bing.test/search")
    );
    assert_eq!(
        errors[0]["fields"]["error"],
        json!("dial https://up.test/x")
    );
    assert!(errors[1]["fields"].get("query").is_none());
    assert_eq!(errors[1]["fields"]["message"], json!("engine slow"));

    // `storage` / `eval_latest`.
    assert!(bundle.sections["storage"]["db_bytes"].is_u64());
    assert_eq!(bundle.sections["storage"]["retention_days"], json!(7));
    assert_eq!(
        bundle.sections["storage"]["logs_files"],
        json!(["cauce-2026-09-28.jsonl"])
    );
    assert!(bundle.sections["eval_latest"].is_null());

    // The serialized export carries no query or secret text at all.
    let out = bundle.to_json();
    for leaked in [
        "My Secret Query",
        "my secret query",
        "my+secret+query",
        "what is my ssn",
        "leak me",
        "sk-test-123",
        "u:p@",
        "Jane Doe",
    ] {
        assert!(!out.contains(leaked), "{leaked:?} leaked:\n{out}");
    }
    // The document round-trips.
    let parsed: ReportBundle = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed.v, 1);
    assert_eq!(parsed.sections.len(), bundle.sections.len());
}

#[tokio::test]
async fn collect_verbose_keeps_queries_only() {
    let _env = env_lock().await;
    let (state, _s, cfg_tmp) = fixture(TOML).await;
    write_logs(&cfg_tmp);
    let store = state.store().clone();
    store
        .log_search(log_row("My Secret Query", 10))
        .await
        .unwrap();
    store
        .audit(AuditRow {
            id: None,
            ts: Utc::now(),
            actor: "Jane Doe <jane@example.com>".into(),
            action: "history.delete".into(),
            target: "42".into(),
            details: json!({"query": "leak me"}),
            request_id: None,
        })
        .await
        .unwrap();

    let bundle = collect(&state, 7, true).await;
    assert_eq!(bundle.profile, cauce_core::RedactionProfile::Verbose);
    let out = bundle.to_json();
    // Query text survives — that is the whole point of the flag.
    for kept in ["My Secret Query", "leak me", "my+secret+query"] {
        assert!(out.contains(kept), "{kept:?} missing:\n{out}");
    }
    // Secrets, credentials and free-form actors still never leave.
    for leaked in ["sk-test-123", "u:p@", "Jane Doe"] {
        assert!(!out.contains(leaked), "{leaked:?} leaked:\n{out}");
    }
    assert_eq!(bundle.sections["config"]["ai"]["api_key"], "<redacted>");
    assert_eq!(bundle.sections["audit_tail"][0]["actor"], "<redacted>");
}

// ---------------------------------------------------------------------------
// `GET /api/report` (#240): the download surface — attachment headers,
// the share URL, safe-by-default and the per-request verbose opt-in.
// ---------------------------------------------------------------------------

/// `fixture` + `build_router` for the route tests.
async fn report_app() -> (axum::Router, tempfile::TempDir, tempfile::TempDir) {
    let (state, store_tmp, cfg_tmp) = fixture(TOML).await;
    let router = cauce_server::build_router(state.clone());
    // Keep the state's TempDir alive through the caller too.
    (router, store_tmp, cfg_tmp)
}

#[tokio::test]
async fn api_report_downloads_a_safe_v1_bundle() {
    let _env = env_lock().await;
    let (router, _s, cfg_tmp) = report_app().await;
    write_logs(&cfg_tmp);

    let (status, headers, body) = get(&router, "/api/report").await;
    assert_eq!(status, StatusCode::OK);
    let cd = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(
        cd.starts_with("attachment; filename=\"cauce-report-") && cd.ends_with(".json\""),
        "content-disposition: {cd}"
    );
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let issue = headers["x-report-issue-url"].to_str().unwrap();
    assert!(
        issue.starts_with("https://github.com/espetro/cauce/issues/new?title="),
        "issue url: {issue}"
    );
    assert_eq!(body["v"], 1);
    assert_eq!(body["profile"], "safe");
    assert!(body["cauce"]["version"].is_string());
}

#[tokio::test]
async fn api_report_days_bounds_the_window() {
    let _env = env_lock().await;
    let (router, _s, cfg_tmp) = report_app().await;
    write_logs(&cfg_tmp);
    // A dated file outside the window is skipped by `errors_tail` and
    // dropped from `storage.logs_files`.
    let logs = cfg_tmp.path().join("data/logs");
    let old = json!({"v": 1, "kind": "event", "ts": "2026-09-01T00:00:00Z",
                     "level": "ERROR", "target": "cauce::old",
                     "fields": {"message": "ancient failure"}});
    std::fs::write(logs.join("cauce-2026-09-01.jsonl"), format!("{old}\n")).unwrap();

    let (status, _h, body) = get(&router, "/api/report?days=1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stats"]["window_days"], 1);
    let files: Vec<&str> = body["storage"]["logs_files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(files, ["cauce-2026-09-28.jsonl"]);
    let tail = body.to_string();
    assert!(!tail.contains("ancient failure"));
    assert!(tail.contains("engine failed"));
}

#[tokio::test]
async fn api_report_verbose_is_explicit_opt_in() {
    let _env = env_lock().await;
    let (router, store_tmp, _c) = report_app().await;
    let _ = store_tmp; // keep the store dir alive for the state's lifetime
    // The default export is `safe`; either flag selects `verbose` once.
    let (_status, _h, body) = get(&router, "/api/report?verbose=1").await;
    assert_eq!(body["profile"], "verbose");
    let (_status, _h, body) = get(&router, "/api/report?include_queries=1").await;
    assert_eq!(body["profile"], "verbose");
    let (_status, _h, body) = get(&router, "/api/report?verbose=0").await;
    assert_eq!(body["profile"], "safe");
}

#[tokio::test]
async fn api_report_rejects_bad_params() {
    let _env = env_lock().await;
    let (router, _s, _c) = report_app().await;
    for uri in [
        "/api/report?days=abc",
        "/api/report?bogus=1",
        "/api/report?verbose=maybe",
    ] {
        let (status, _h, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
    }
}
