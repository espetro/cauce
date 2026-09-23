//! `cauce search` CLI e2e (W2-11).
//!
//! The subcommand runs the pipeline in-process against the same
//! `CAUCE_DATA_DIR` SQLite DB `cauce serve` opens — no server needed — and
//! prints the `request_id` on stderr so stdout stays parseable.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

use crate::common;

/// One `cauce search` invocation isolated to the given config/data dirs.
fn search(config_dir: &Path, data_dir: &Path, args: &[&str]) -> Output {
    Command::new(common::cauce_bin())
        .current_dir(common::workspace_root())
        .arg("search")
        .args(args)
        .env("CAUCE_CONFIG_DIR", config_dir)
        .env("CAUCE_DATA_DIR", data_dir)
        .env("CAUCE_ENGINES", "replay")
        .output()
        .expect("spawn cauce search")
}

fn dirs() -> (TempDir, TempDir) {
    (
        TempDir::new().expect("config dir"),
        TempDir::new().expect("data dir"),
    )
}

/// Acceptance: `cauce search --json x` on replay prints a `SearchResponse`.
#[test]
fn search_json_prints_search_response() {
    let (config_dir, data_dir) = dirs();
    let out = search(
        config_dir.path(),
        data_dir.path(),
        &["--json", "golden cli"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);

    let resp: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not a SearchResponse: {e}: {stdout}"));
    assert_eq!(resp["query"], "golden cli");
    assert!(
        resp["results"].as_array().unwrap().len() >= 5,
        "replay should yield results: {resp}"
    );
    assert_eq!(resp["meta"]["source"], "network");
    let request_id = resp["meta"]["request_id"]
        .as_str()
        .expect("meta.request_id");
    assert!(request_id.parse::<uuid::Uuid>().is_ok());

    // The request id goes to stderr and matches meta.request_id.
    assert!(
        stderr.contains(&format!("request_id: {request_id}")),
        "stderr should carry the request id: {stderr}"
    );
}

/// A positional query containing `=` remains intact after option parsing.
#[test]
fn search_query_with_equals_is_not_treated_as_inline_option() {
    let (config_dir, data_dir) = dirs();
    let out = search(config_dir.path(), data_dir.path(), &["--json", "x=y"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);
    let response: Value = serde_json::from_str(&stdout).expect("SearchResponse");
    assert_eq!(response["query"], "x=y");
}

/// `--urls` prints bare URLs (script-friendly); `--table` prints aligned
/// rows; the flags are mutually exclusive.
#[test]
fn search_urls_and_table_formats() {
    let (config_dir, data_dir) = dirs();

    let out = search(config_dir.path(), data_dir.path(), &["--urls", "urls fmt"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 5, "expected URL lines: {stdout}");
    for line in &lines {
        assert!(
            line.starts_with("http://") || line.starts_with("https://"),
            "--urls line is not a bare URL: {line:?}"
        );
    }

    let out = search(
        config_dir.path(),
        data_dir.path(),
        &["--table", "table fmt"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(
        stdout.contains("TITLE") && stdout.contains("ENGINE"),
        "table should print a header row: {stdout}"
    );
    assert!(
        stdout.contains("http"),
        "table rows should carry URLs: {stdout}"
    );

    let out = search(
        config_dir.path(),
        data_dir.path(),
        &["--json", "--urls", "x"],
    );
    assert_eq!(out.status.code(), Some(2), "conflicting formats must fail");
}

/// `--engines` pins the fan-out; an unknown pin exits non-zero.
#[test]
fn search_engines_pin() {
    let (config_dir, data_dir) = dirs();
    let out = search(
        config_dir.path(),
        data_dir.path(),
        &["--json", "--engines=replay", "pinned"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    let resp: Value = serde_json::from_str(&stdout).expect("SearchResponse");
    assert_eq!(resp["meta"]["engines_used"][0]["engine"], "replay");

    let out = search(
        config_dir.path(),
        data_dir.path(),
        &["--engines", "nope", "pinned"],
    );
    assert!(
        !out.status.success(),
        "unknown engine pin should fail: {out:?}"
    );
}

/// "Shares the DB": a query already run through the HTTP surface is a
/// tier-1 cache hit when `cauce search` opens the same data dir.
#[tokio::test]
async fn search_shares_db_with_server() {
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let server = common::spawn_cauce(&data_dir, &config_dir, "replay").await;
    let (status, body) = common::http(server.addr, "GET", "/api/search?q=shared", None).await;
    assert_eq!(status, 200, "{body}");

    let out = search(&config_dir, &data_dir, &["--json", "shared"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);
    let resp: Value = serde_json::from_str(&stdout).expect("SearchResponse");
    assert_eq!(
        resp["meta"]["source"]["cache"]["tier"], 1,
        "second surface should be a tier-1 cache hit: {resp}"
    );

    server.shutdown().await.expect("shutdown");
}
