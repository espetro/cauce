//! W3-06 acceptance: `cauce engine test --live` is the nightly drift
//! canary. A spec whose upstream stops emitting the committed fixture's
//! selector exits non-zero — simulated here against a mock upstream on
//! loopback (the same `wiremock` pattern the engine-runtime tests use),
//! never the real network.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;

use tokio::process::Command;

use crate::common::{cauce_bin, workspace_root};

/// Spec `id` shared by the test spec and its `<fixtures-dir>/<id>/` dir.
const SPEC_ID: &str = "canaryspec";

/// A spec whose `request.url` pages via `first={offset+1}` (page 2 ->
/// first=11), pointed at the mock upstream.
fn spec_yaml(base_uri: &str) -> String {
    format!(
        r#"
id: {SPEC_ID}
tier: 1
page_size: 10
request:
  url: "{base_uri}/s?q={{q}}&first={{offset+1}}"
  timeout_ms: 5000
parse:
  kind: html
  results: "div.result"
  fields:
    title: {{ css: "h2.t", text: true }}
    url: {{ css: "a.u", attr: href }}
    snippet: {{ css: "p.s", text: true }}
"#
    )
}

/// `<fixtures>/<id>/` with one committed page-1 fixture (body +
/// `.expected.json`) asserting `n` fully-populated results.
fn write_baseline(fixtures_root: &Path, n: usize) {
    let dir = fixtures_root.join(SPEC_ID);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("baseline-00000000.html"), "<html></html>").unwrap();
    let results: Vec<serde_json::Value> = (0..n)
        .map(|i| {
            serde_json::json!({
                "title": format!("fixture title {i}"),
                "url": format!("https://example.com/f{i}"),
                "snippet": format!("fixture snippet {i}"),
            })
        })
        .collect();
    let expected = serde_json::json!({
        "query": "drift canary",
        "page": 1,
        "status": 200,
        "results": results,
    });
    std::fs::write(
        dir.join("baseline-00000000.expected.json"),
        format!("{}\n", serde_json::to_string_pretty(&expected).unwrap()),
    )
    .unwrap();
}

/// `n` `div.result` rows with `pfx`-namespaced urls (what the spec
/// selects on).
fn serp(n: usize, pfx: &str) -> String {
    let mut body = String::from("<html><body>");
    for i in 0..n {
        body.push_str(&format!(
            r#"<div class="result"><h2 class="t">title {i}</h2>\
             <a class="u" href="https://example.com/{pfx}{i}">link</a>\
             <p class="s">snippet {i}</p></div>"#
        ));
    }
    body.push_str("</body></html>");
    body
}

/// Spawn `cauce engine test <spec> --live` hermetically (temp config/data
/// dirs so no real `config.toml` or egress table leaks in).
async fn run_canary(spec_path: &Path, fixtures_root: &Path) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(cauce_bin())
        .current_dir(workspace_root())
        .arg("engine")
        .arg("test")
        .arg(spec_path)
        .arg("--live")
        .arg("--fixtures-dir")
        .arg(fixtures_root)
        .env("CAUCE_CONFIG_DIR", tmp.path().join("config"))
        .env("CAUCE_DATA_DIR", tmp.path().join("data"))
        .output()
        .await
        .expect("spawn cauce engine test --live")
}

/// A spec whose committed fixture selects `div.result` but whose upstream
/// now serves a page without any of them: page 1 parse fails and the
/// canary exits non-zero.
#[tokio::test]
async fn live_canary_exits_nonzero_on_removed_selector() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/s"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string("<html><body><main>new layout</main></body></html>"),
        )
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let spec_path = tmp.path().join("spec.yaml");
    std::fs::write(&spec_path, spec_yaml(&server.uri())).unwrap();
    let fixtures = tmp.path().join("fixtures");
    write_baseline(&fixtures, 10);

    let output = run_canary(&spec_path, &fixtures).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "removed-selector canary must exit non-zero, stdout:\n{stdout}"
    );
    assert!(stdout.contains("FAIL"), "stdout:\n{stdout}");
}

/// Healthy page 1 plus a page 2 re-serving the same urls trips the
/// 'serves page 1 again' anti-bot check (SearXNG #3402/#4546).
#[tokio::test]
async fn live_canary_exits_nonzero_on_page2_overlap() {
    let server = wiremock::MockServer::start().await;
    for page in ["1", "11"] {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/s"))
            .and(wiremock::matchers::query_param("first", page))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(serp(10, "f")))
            .mount(&server)
            .await;
    }

    let tmp = tempfile::tempdir().unwrap();
    let spec_path = tmp.path().join("spec.yaml");
    std::fs::write(&spec_path, spec_yaml(&server.uri())).unwrap();
    let fixtures = tmp.path().join("fixtures");
    write_baseline(&fixtures, 10);

    let output = run_canary(&spec_path, &fixtures).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "page-2 replay must exit non-zero, stdout:\n{stdout}"
    );
    assert!(stdout.contains("page-1 urls"), "stdout:\n{stdout}");
}

/// A healthy upstream (full page 1, distinct page-2 urls) passes.
#[tokio::test]
async fn live_canary_passes_on_healthy_upstream() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/s"))
        .and(wiremock::matchers::query_param("first", "1"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(serp(10, "f")))
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/s"))
        .and(wiremock::matchers::query_param("first", "11"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(serp(10, "g")))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let spec_path = tmp.path().join("spec.yaml");
    std::fs::write(&spec_path, spec_yaml(&server.uri())).unwrap();
    let fixtures = tmp.path().join("fixtures");
    write_baseline(&fixtures, 10);

    let output = run_canary(&spec_path, &fixtures).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "healthy canary should exit 0, stdout:\n{stdout}"
    );
    assert!(stdout.contains("PASS"), "stdout:\n{stdout}");
}

/// A spec with no committed fixture cannot be canaried — that is itself a
/// failure (every shipped spec ships a fixture).
#[tokio::test]
async fn live_canary_fails_without_a_baseline_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let spec_path = tmp.path().join("spec.yaml");
    std::fs::write(&spec_path, spec_yaml("http://127.0.0.1:1")).unwrap();
    let fixtures = tmp.path().join("fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();

    let output = run_canary(&spec_path, &fixtures).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success());
    assert!(
        stdout.contains("no committed page-1 fixture"),
        "stdout:\n{stdout}"
    );
}

/// W3-06: the nightly canary workflow exists and triggers on `cron`
/// only — it can never gate merges, and the failed run is the report.
#[test]
fn engine_canary_workflow_is_cron_only() {
    let path = workspace_root().join(".github/workflows/engine-canary.yml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let on_block = text.split("jobs:").next().expect("workflow has jobs");
    assert!(on_block.contains("schedule:") && on_block.contains("cron:"));
    for trigger in ["pull_request", "push:", "workflow_dispatch", "merge_group"] {
        assert!(
            !on_block.contains(trigger),
            "nightly canary must not trigger on {trigger:?}"
        );
    }
    // Unlike the evals workflow, the canary opens no tracking issue.
    assert!(!text.contains("gh issue"), "the run itself is the report");
}
