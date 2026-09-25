//! W3-05 acceptance: `cauce eval engines` on replay cassettes scores 100 %
//! for a case whose cassette contains the expected domain, and the nightly
//! workflow file exists with a `cron`-only trigger.
//!
//! W4-04 acceptance: `cauce eval ai --tag smoke --gate` runs the five
//! committed smoke cases over recorded transcripts + replay cassettes and
//! exits 0 at score 1.0 (the CI gate's exact invocation); a doctored
//! case below `baseline - tolerance` exits 1; a missing transcript is a
//! 0-scored outcome with a note, never a silent skip.
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

/// The CI gate's exact invocation: the five `smoke`-tagged committed
/// cases over recorded transcripts + replay cassettes — six outcomes
/// because `tokyo-weather` also has an `anthropic` protocol variant
/// (W4-05). Exit 0 at score 1.0 and the report lands as
/// `<date>-ai.json` — this is what `.github/workflows/validate.yml`
/// runs after `mise run validate`.
#[tokio::test]
async fn eval_ai_smoke_gate_passes_on_committed_cases() {
    let root = workspace_root();
    let results_dir = tempfile::tempdir().unwrap();
    let output = Command::new(cauce_bin())
        .current_dir(root)
        .arg("eval")
        .arg("ai")
        .arg(root.join("evals/ai/smoke.jsonl"))
        .arg("--tag")
        .arg("smoke")
        .arg("--gate")
        .arg("--results-dir")
        .arg(results_dir.path())
        .output()
        .await
        .expect("spawn cauce eval ai");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "smoke gate should exit 0\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let entry = std::fs::read_dir(results_dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .next()
        .expect("one report file")
        .file_name()
        .into_string()
        .unwrap();
    assert!(entry.ends_with("-ai.json"), "report name: {entry}");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(results_dir.path().join(entry)).unwrap())
            .unwrap();
    assert_eq!(report["kind"], "ai");
    assert_eq!(
        report["cases"], 6,
        "five smoke cases, six outcomes (tokyo-weather runs both protocols): {report}"
    );
    assert_eq!(report["score"], 1.0, "committed cases score 1.0: {report}");
    assert_eq!(report["gate_ok"], true);
    // Deterministic ordering: outcomes stay in case-file order, with a
    // case's protocol variants adjacent (openai first).
    let outcomes = report["outcomes"].as_array().unwrap();
    let queries: Vec<_> = outcomes
        .iter()
        .map(|o| o["query"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        queries.first().unwrap(),
        "current weather in Tokyo right now"
    );
    assert_eq!(outcomes[0]["protocol"], "openai");
    assert_eq!(
        outcomes[1]["protocol"], "anthropic",
        "the anthropic transcript variant runs as a second outcome: {report}"
    );
    // The ungrounded metric reports the no-tools case without gating.
    assert_eq!(
        report["ungrounded_cases"],
        serde_json::json!(["what is the rust programming language"])
    );
}

/// `--gate` fails (exit 1) when the score drops below
/// `baseline - tolerance` — a case whose `must_contain` cannot be met
/// stands in for a regressed answer.
#[tokio::test]
async fn eval_ai_gate_fails_below_baseline() {
    let root = workspace_root();
    let tmp = tempfile::tempdir().unwrap();
    let cases = tmp.path().join("cases.jsonl");
    std::fs::write(
        &cases,
        "{\"query\": \"current weather in Tokyo right now\", \"transcript\": \"tokyo-weather\", \"must_cite_domains\": [\"jma.go.jp\"], \"must_contain\": [\"never-present-xyzzy\"], \"must_not_contain\": [\"related_questions\"], \"tags\": [\"smoke\"]}\n",
    )
    .unwrap();
    let thresholds = tmp.path().join("thresholds.toml");
    std::fs::write(&thresholds, "[ai]\nbaseline = 1.0\ntolerance = 0.0\n").unwrap();

    let output = Command::new(cauce_bin())
        .current_dir(root)
        .arg("eval")
        .arg("ai")
        .arg(&cases)
        .arg("--tag")
        .arg("smoke")
        .arg("--gate")
        .arg("--thresholds")
        .arg(&thresholds)
        .arg("--results-dir")
        .arg(tmp.path().join("results"))
        .output()
        .await
        .expect("spawn cauce eval ai");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "gate should fail below baseline, stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("gate failed"),
        "stderr should explain the gate failure: {stderr}"
    );
}

/// A case whose transcript file is missing is a 0-scored outcome with
/// the load error as its note — never a silent skip that would inflate
/// the mean.
#[tokio::test]
async fn eval_ai_missing_transcript_scores_zero_with_note() {
    let root = workspace_root();
    let tmp = tempfile::tempdir().unwrap();
    let cases = tmp.path().join("cases.jsonl");
    std::fs::write(
        &cases,
        "{\"query\": \"a question\", \"transcript\": \"no-such-transcript\", \"must_cite_domains\": [\"a.com\"], \"must_contain\": [\"x\"], \"must_not_contain\": [], \"tags\": []}\n",
    )
    .unwrap();
    let results_dir = tmp.path().join("results");

    let output = Command::new(cauce_bin())
        .current_dir(root)
        .arg("eval")
        .arg("ai")
        .arg(&cases)
        .arg("--results-dir")
        .arg(&results_dir)
        .output()
        .await
        .expect("spawn cauce eval ai");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "no --gate: a bad case is a report outcome, not a CLI failure\nstderr:\n{stderr}"
    );
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
    assert_eq!(report["score"], 0.0);
    assert_eq!(report["outcomes"][0]["ok"], false);
    assert!(
        report["outcomes"][0]["note"]
            .as_str()
            .unwrap()
            .contains("transcript"),
        "missing transcript should be noted: {report}"
    );
}

/// `cauce eval` on its own prints combined usage; an unknown kind is a
/// usage error (exit 2).
#[tokio::test]
async fn eval_usage_and_unknown_kind() {
    let output = Command::new(cauce_bin())
        .arg("eval")
        .output()
        .await
        .expect("spawn cauce eval");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("engines") && stdout.contains("ai"),
        "{stdout}"
    );

    let output = Command::new(cauce_bin())
        .arg("eval")
        .arg("bogus")
        .output()
        .await
        .expect("spawn cauce eval bogus");
    assert_eq!(output.status.code(), Some(2));
}
