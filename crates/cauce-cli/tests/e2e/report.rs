//! #240/#242: `cauce report` end-to-end — the CLI assembles the same
//! v1 bundle `GET /api/report` serves, isolated to temp config/data dirs
//! (no server needed).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

use crate::common;

/// One `cauce report` invocation isolated to the given config/data dirs,
/// writing into `cwd` (a tempdir, so default `--out` artifacts do not
/// litter the workspace).
fn report(config_dir: &Path, data_dir: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(common::cauce_bin())
        .current_dir(cwd)
        .arg("report")
        .args(args)
        .env("CAUCE_CONFIG_DIR", config_dir)
        .env("CAUCE_DATA_DIR", data_dir)
        .env("CAUCE_ENGINES", "replay")
        .env("CAUCE_EVAL_RESULTS_DIR", cwd.join("evals-results"))
        .output()
        .expect("spawn cauce report")
}

fn dirs() -> (TempDir, TempDir, TempDir) {
    (
        TempDir::new().expect("config dir"),
        TempDir::new().expect("data dir"),
        TempDir::new().expect("cwd"),
    )
}

fn assert_bundle(text: &str) -> Value {
    let bundle: Value =
        serde_json::from_str(text).unwrap_or_else(|e| panic!("not a ReportBundle: {e}: {text}"));
    assert_eq!(bundle["v"], 1);
    assert!(bundle["generated_at"].is_string());
    assert!(bundle["cauce"]["version"].is_string());
    bundle
}

/// `cauce report --print` writes the bundle to stdout, statuses to stderr.
#[test]
fn report_print_emits_bundle_on_stdout() {
    let (cfg, data, cwd) = dirs();
    let out = report(cfg.path(), data.path(), cwd.path(), &["--print"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);

    let bundle = assert_bundle(&stdout);
    assert_eq!(bundle["profile"], "safe");
    assert!(
        stderr.contains("issue: https://github.com/espetro/cauce/issues/new?title="),
        "stderr missing the prefilled-issue URL: {stderr}"
    );
}

/// `cauce report` (no flags) writes `cauce-report-<ts>.json` under cwd.
#[test]
fn report_default_writes_dated_file() {
    let (cfg, data, cwd) = dirs();
    let out = report(cfg.path(), data.path(), cwd.path(), &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);

    let files: Vec<_> = std::fs::read_dir(cwd.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("cauce-report-") && n.ends_with(".json"))
        .collect();
    assert_eq!(files.len(), 1, "cwd files: {files:?}");
    let bundle = assert_bundle(&std::fs::read_to_string(cwd.path().join(&files[0])).unwrap());
    assert_eq!(bundle["profile"], "safe");
    assert!(
        stdout.contains("wrote ") && stdout.contains("issue: "),
        "stdout: {stdout}"
    );
}

/// `--out` writes the given path; `--include-queries` selects verbose
/// for that export only.
#[test]
fn report_out_and_include_queries() {
    let (cfg, data, cwd) = dirs();
    let path = cwd.path().join("verbose.json");
    let out = report(
        cfg.path(),
        data.path(),
        cwd.path(),
        &["--out", path.to_str().unwrap(), "--include-queries"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "exit {}: {stderr}", out.status);
    let bundle = assert_bundle(&std::fs::read_to_string(&path).unwrap());
    assert_eq!(bundle["profile"], "verbose");
}

/// Bad flags and bad values are usage errors (exit 2), not crashes.
#[test]
fn report_rejects_bad_args() {
    let (cfg, data, cwd) = dirs();
    for args in [
        vec!["--bogus"],
        vec!["--days", "abc"],
        vec!["--days", "0"],
        vec!["--out"],
    ] {
        let out = report(cfg.path(), data.path(), cwd.path(), &args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// `-h`/`--help` print usage to stdout and exit 0.
#[test]
fn report_help_exits_zero() {
    let (cfg, data, cwd) = dirs();
    for flag in ["-h", "--help"] {
        let out = report(cfg.path(), data.path(), cwd.path(), &[flag]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{flag}: {stdout}");
        assert!(stdout.contains("usage: cauce report"), "{flag}: {stdout}");
    }
}
