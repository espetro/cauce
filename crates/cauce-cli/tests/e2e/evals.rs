//! W3-05 acceptance: `cauce eval engines` on replay cassettes scores 100 %
//! for a case whose cassette contains the expected domain, and the nightly
//! workflow file exists with a `cron`-only trigger.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use tokio::process::Command;

use cauce_engines::cassette_path;

use crate::common::{cauce_bin, workspace_root};

/// A cassette-shaped JSON document for `engine` serving `query`, with
/// `hosts` becoming the result URLs (`https://<host>/<n>`).
fn cassette_json(engine: &str, query: &str, hosts: &[&str]) -> serde_json::Value {
    let results: Vec<_> = hosts
        .iter()
        .enumerate()
        .map(|(i, host)| {
            serde_json::json!({
                "url": format!("https://{host}/r{i}"),
                "title": format!("result {i}"),
                "snippet": "recorded",
                "engine": engine,
                "score": 1.0,
            })
        })
        .collect();
    serde_json::json!({
        "query": query,
        "engine": engine,
        "recorded_at": "2026-09-24T00:00:00Z",
        "results": results,
    })
}

/// `cauce eval engines` on replay cassettes: a case whose cassette serves
/// the expected domain in the top 5 scores 100 %, and the report lands at
/// `<results_dir>/<date>-engines.json`.
#[tokio::test]
async fn eval_engines_replay_scores_cassette_hit() {
    let tmp = tempfile::tempdir().unwrap();
    let fixtures = tmp.path().join("fixtures");
    let results_dir = tmp.path().join("results");
    let query = "eval acceptance query";

    let path = cassette_path(&fixtures, "bing", query);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // The expected domain sits at rank 2 of five — inside the @5 window.
    let cassette = cassette_json(
        "bing",
        query,
        &[
            "example.org",
            "tanstack.com",
            "another.net",
            "third.io",
            "fourth.dev",
        ],
    );
    std::fs::write(&path, serde_json::to_string_pretty(&cassette).unwrap()).unwrap();

    let cases = tmp.path().join("cases.jsonl");
    std::fs::write(
        &cases,
        format!(
            "{{\"query\": {query:?}, \"expect_domains_top5\": [\"tanstack.com\"], \"engines\": [\"bing\"]}}\n"
        ),
    )
    .unwrap();
    let thresholds = tmp.path().join("thresholds.toml");
    std::fs::write(&thresholds, "default = 0.8\n").unwrap();

    let output = Command::new(cauce_bin())
        .arg("eval")
        .arg("engines")
        .arg(&cases)
        .arg("--fixtures-dir")
        .arg(&fixtures)
        .arg("--results-dir")
        .arg(&results_dir)
        .arg("--thresholds")
        .arg(&thresholds)
        .output()
        .await
        .expect("spawn cauce eval");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "eval should exit 0, stderr:\n{stderr}"
    );

    let entries: Vec<_> = std::fs::read_dir(&results_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    assert_eq!(entries.len(), 1, "one report file expected: {entries:?}");
    assert!(entries[0].ends_with("-engines.json"));

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(results_dir.join(&entries[0])).unwrap())
            .unwrap();
    assert_eq!(report["kind"], "engines");
    assert_eq!(report["live"], false);
    assert_eq!(report["engines"][0]["engine"], "bing");
    assert_eq!(
        report["engines"][0]["domain_hit_at5"], 1.0,
        "cassette contains the expected domain: {report}"
    );
    assert_eq!(report["engines"][0]["ok"], true);
    assert_eq!(
        report["below_threshold"],
        serde_json::json!([]),
        "nothing below threshold: {report}"
    );
    assert_eq!(report["outcomes"][0]["hit"], true);
    assert!(
        report["outcomes"][0]["top5_hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h == "tanstack.com"),
        "top5 hosts recorded for review: {report}"
    );
}

/// A query with no cassette is a miss with a `no cassette` note — never a
/// synthetic page that could fake a hit — and it lands the engine on
/// `below_threshold`.
#[tokio::test]
async fn eval_engines_replay_missing_cassette_is_a_miss() {
    let tmp = tempfile::tempdir().unwrap();
    let fixtures = tmp.path().join("fixtures");
    let results_dir = tmp.path().join("results");
    std::fs::create_dir_all(&fixtures).unwrap();

    let cases = tmp.path().join("cases.jsonl");
    std::fs::write(
        &cases,
        "{\"query\": \"no cassette here\", \"expect_domains_top5\": [\"tanstack.com\"], \"engines\": [\"bing\"]}\n",
    )
    .unwrap();
    let thresholds = tmp.path().join("thresholds.toml");
    std::fs::write(&thresholds, "default = 0.8\n").unwrap();

    let output = Command::new(cauce_bin())
        .arg("eval")
        .arg("engines")
        .arg(&cases)
        .arg("--fixtures-dir")
        .arg(&fixtures)
        .arg("--results-dir")
        .arg(&results_dir)
        .arg("--thresholds")
        .arg(&thresholds)
        .output()
        .await
        .expect("spawn cauce eval");
    assert!(output.status.success());

    let entry = std::fs::read_dir(&results_dir)
        .unwrap()
        .filter_map(Result::ok)
        .next()
        .unwrap()
        .file_name()
        .into_string()
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(results_dir.join(entry)).unwrap()).unwrap();
    assert_eq!(report["engines"][0]["domain_hit_at5"], 0.0);
    assert_eq!(report["engines"][0]["ok"], false);
    assert_eq!(report["below_threshold"], serde_json::json!(["bing"]));
    assert_eq!(report["outcomes"][0]["hit"], false);
    assert!(
        report["outcomes"][0]["note"]
            .as_str()
            .unwrap()
            .contains("no cassette"),
        "missing cassette should be noted: {report}"
    );
}

/// W3-05 acceptance: the nightly eval workflow exists and triggers on
/// `cron` only — it can never gate merges.
#[test]
fn engine_evals_workflow_is_cron_only() {
    let path = workspace_root().join(".github/workflows/engine-evals.yml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let on_block = text
        .split("jobs:")
        .next()
        .expect("workflow has a jobs section");
    assert!(on_block.contains("schedule:") && on_block.contains("cron:"));
    for trigger in ["pull_request", "push:", "workflow_dispatch", "merge_group"] {
        assert!(
            !on_block.contains(trigger),
            "nightly evals must not trigger on {trigger:?}"
        );
    }
}
