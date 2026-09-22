//! W1-02 acceptance tests for the declarative runtime:
//!
//! * a synthetic spec + HTML fixture parses 10 results;
//! * a fixture whose selectors match nothing yields
//!   `EngineError::Parse("0 results, selector ...")`, not an empty `Ok`;
//! * a blocked-substring fixture yields `EngineError::Blocked`.
//!
//! Plus the contract surface around them: request templating, header
//! `${env:}` interpolation, `detect.rate_limited_status`, `parse.kind:
//! json` (including parallel-array APIs), redirect unwrapping inside
//! result urls, fixture-pair running/recording, spec loading precedence
//! and the `Engine::search` fetch+parse path over a mock upstream.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::time::Duration;

use oxe_core::config::{Config, EngineEntry, EngineKind, EnvMap};
use oxe_core::http::HttpClient;
use oxe_core::{ClientKind, Engine, EngineError, SafeSearch, SearchRequest, Tier};
use oxe_engines::declarative::fixtures::{
    ExpectedFixture, ExpectedResult, fixture_pairs, run_pair, write_pair,
};
use oxe_engines::declarative::{CompiledSpec, DeclarativeEngine, load_specs, resolve_spec_source};
use oxe_engines::factory::{build_engine, build_engines};

const SPEC_YAML: &str = r#"
id: fixture
tier: 1
page_size: 10
request:
  url: "https://search.test.local/s?q={q}&p={page0}&first={offset+1}"
  headers:
    Accept-Language: "{lang}"
    X-Key: "${env:OXE_TEST_SPEC_KEY:-no-key}"
  timeout_ms: 5000
parse:
  kind: html
  results: "div.result"
  fields:
    title: { css: "h2.t", text: true }
    url: { css: "a.u", attr: href }
    snippet: { css: [".b_caption p", "p.note"], text: true }
detect:
  blocked: ["unusual traffic"]
  rate_limited_status: [429]
unwrap_redirect:
  - match: "bing.com/ck/a"
    param: u
    strip: "a1"
    base64: true
"#;

fn env() -> EnvMap {
    BTreeMap::new()
}

fn spec() -> CompiledSpec {
    CompiledSpec::from_yaml(SPEC_YAML, &env()).unwrap()
}

fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Cli,
    }
}

fn base(spec: &CompiledSpec, q: &str) -> url::Url {
    spec.render_url(&req(q)).unwrap()
}

/// `n` synthetic results inside `div.result` blocks.
fn html_body(n: usize) -> String {
    let mut body = String::from("<html><body>");
    for i in 0..n {
        body.push_str(&format!(
            r#"<div class="result"><h2 class="t">  Title   {i}
               multiline </h2><a class="u" href="/item/{i}?utm_source=x">link</a>
               <p class="note"> snippet {i} </p></div>"#
        ));
    }
    body.push_str("</body></html>");
    body
}

#[test]
fn html_fixture_parses_ten_results() {
    let spec = spec();
    let base = base(&spec, "tanstack router docs");
    let results = spec
        .parse_response(200, html_body(10).as_bytes(), &base)
        .unwrap();
    assert_eq!(results.len(), 10);
    let first = &results[0];
    // Whitespace is collapsed, relative urls resolve against the request
    // url, tracking params are stripped by normalize_url, engine id and a
    // positional score are set.
    assert_eq!(first.title, "Title 0 multiline");
    assert_eq!(first.url.as_str(), "https://search.test.local/item/0");
    assert_eq!(first.snippet, "snippet 0");
    assert_eq!(first.engine.as_str(), "fixture");
    assert_eq!(first.score, 1.0);
    assert!(results[1].score < results[0].score);
}

#[test]
fn zero_match_selector_yields_parse_error_not_empty_ok() {
    let spec = spec();
    let base = base(&spec, "q");
    let err = spec
        .parse_response(200, b"<html><body><p>nothing here</p></body></html>", &base)
        .unwrap_err();
    match err {
        EngineError::Parse(msg) => {
            assert!(msg.starts_with("0 results, selector"), "{msg}");
            assert!(msg.contains("div.result"), "{msg}");
        }
        other => panic!("expected EngineError::Parse, got {other:?}"),
    }
}

#[test]
fn blocked_substring_fixture_yields_blocked() {
    let spec = spec();
    let base = base(&spec, "q");
    let err = spec
        .parse_response(
            200,
            b"<html><body>Our systems detected Unusual Traffic from your network</body></html>",
            &base,
        )
        .unwrap_err();
    assert_eq!(err, EngineError::Blocked);
}

#[test]
fn rate_limited_status_maps_before_parse() {
    let spec = spec();
    let base = base(&spec, "q");
    let err = spec.parse_response(429, b"{}", &base).unwrap_err();
    assert_eq!(err, EngineError::RateLimited);
}

#[test]
fn other_error_status_is_transport() {
    let spec = spec();
    let base = base(&spec, "q");
    let err = spec
        .parse_response(500, b"<div class='result'></div>", &base)
        .unwrap_err();
    assert!(matches!(err, EngineError::Transport(m) if m.contains("500")));
}

#[test]
fn request_templating_renders_all_tokens() {
    let spec = spec();
    let mut r = req("a b&c/d?");
    r.page = 3;
    r.lang = Some("de".to_string());
    let url = spec.render_url(&r).unwrap();
    // `{q}` form-urlencodes (space -> `+`), `{page0}` = page-1,
    // `{offset+1}` = (page-1)*page_size + 1, `{lang}` verbatim.
    assert_eq!(
        url.as_str(),
        "https://search.test.local/s?q=a+b%26c%2Fd%3F&p=2&first=21"
    );
    let headers = spec.render_headers(&r).unwrap();
    assert_eq!(headers["accept-language"], "de");
    // `${env:...:-default}` resolved at compile time.
    assert_eq!(headers["x-key"], "no-key");
}

#[test]
fn header_env_interpolation_uses_env() {
    let env: EnvMap = BTreeMap::from([("OXE_TEST_SPEC_KEY".to_string(), "s3cret".to_string())]);
    let spec = CompiledSpec::from_yaml(SPEC_YAML, &env).unwrap();
    let headers = spec.render_headers(&req("q")).unwrap();
    assert_eq!(headers["x-key"], "s3cret");

    // A required `${env:...}` without a default fails compilation.
    let yaml = SPEC_YAML.replace(":-no-key", "");
    let env_missing: EnvMap = BTreeMap::new();
    assert!(CompiledSpec::from_yaml(&yaml, &env_missing).is_err());
}

#[test]
fn bing_redirect_inside_results_is_unwrapped() {
    use base64::Engine as _;
    let target = "https://real.example/page";
    let u = format!(
        "a1{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(target)
    );
    let body = format!(
        r#"<html><body><div class="result"><h2 class="t">T</h2>
           <a class="u" href="https://www.bing.com/ck/a?x=1&u={u}">l</a>
           <p class="note">s</p></div></body></html>"#
    );
    let spec = spec();
    let results = spec
        .parse_response(200, body.as_bytes(), &base(&spec, "q"))
        .unwrap();
    assert_eq!(results[0].url.as_str(), target);
}

#[test]
fn non_http_scheme_result_urls_are_dropped() {
    use base64::Engine as _;
    // A crafted `javascript:`/`data:` href, or a bing `u=` payload
    // decoding to `javascript:`, must never land in a `SearchResult.url`
    // that W2 renders as `<a href>`.
    let js_payload = format!(
        "a1{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("javascript:alert(1)")
    );
    let body = format!(
        r#"<html><body>
           <div class="result"><h2 class="t">JS</h2>
             <a class="u" href="javascript:alert(1)">l</a><p class="note">s</p></div>
           <div class="result"><h2 class="t">DATA</h2>
             <a class="u" href="data:text/html;base64,PHNjcmlwdD4=">l</a><p class="note">s</p></div>
           <div class="result"><h2 class="t">FILE</h2>
             <a class="u" href="file:///etc/passwd">l</a><p class="note">s</p></div>
           <div class="result"><h2 class="t">REDIR-JS</h2>
             <a class="u" href="https://www.bing.com/ck/a?u={js_payload}">l</a><p class="note">s</p></div>
           <div class="result"><h2 class="t">OK</h2>
             <a class="u" href="/good">l</a><p class="note">s</p></div>
           </body></html>"#
    );
    let spec = spec();
    let results = spec
        .parse_response(200, body.as_bytes(), &base(&spec, "q"))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url.as_str(), "https://search.test.local/good");
}

#[test]
fn json_kind_parallel_arrays() {
    let yaml = r#"
id: wiki
request:
  url: "https://{lang}.example.org/api?search={q}"
parse:
  kind: json
  results: "$[1]"
  fields:
    title: { path: "$[1][{i}]", root: true }
    url: { path: "$[3][{i}]", root: true }
    snippet: { path: "$[2][{i}]", root: true }
"#;
    let spec = CompiledSpec::from_yaml(yaml, &env()).unwrap();
    let body = r#"["rust",["Rust (lang)","Rust (game)"],["a systems lang","a survival game"],["https://a/1","https://a/2"]]"#;
    let base = spec.render_url(&req("rust")).unwrap();
    let results = spec.parse_response(200, body.as_bytes(), &base).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].title, "Rust (lang)");
    assert_eq!(results[1].url.as_str(), "https://a/2");
    assert_eq!(results[1].snippet, "a survival game");
}

#[test]
fn json_kind_object_array() {
    let yaml = r#"
id: j
request:
  url: "https://api.example/s?q={q}"
parse:
  kind: json
  results: "$.items[*]"
  fields:
    title: { path: "$.t" }
    url: { path: "$.u" }
    snippet: { path: "$.s" }
"#;
    let spec = CompiledSpec::from_yaml(yaml, &env()).unwrap();
    let body =
        r#"{"items":[{"t":"A","u":"https://a","s":"sa"},{"t":"B","u":"https://b","s":"sb"}]}"#;
    let base = spec.render_url(&req("q")).unwrap();
    let results = spec.parse_response(200, body.as_bytes(), &base).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[1].title, "B");
}

#[test]
fn spec_validation_rejects_bad_input() {
    let env = env();
    for (label, yaml) in [
        ("unknown field", SPEC_YAML.replace("snippet:", "snippit:")),
        ("bad css", SPEC_YAML.replace("div.result", "div[[")),
        (
            "missing url field",
            SPEC_YAML.replace("url: { css: \"a.u\", attr: href }\n    ", ""),
        ),
        ("bad placeholder", SPEC_YAML.replace("q={q}", "q={query}")),
    ] {
        assert!(
            CompiledSpec::from_yaml(&yaml, &env).is_err(),
            "{label} should fail to compile"
        );
    }
}

#[test]
fn fixture_pair_round_trip() {
    let spec = spec();
    let tmp = tempfile::tempdir().unwrap();
    let results = spec
        .parse_response(200, html_body(3).as_bytes(), &base(&spec, "rust lang"))
        .unwrap();
    let body_path = write_pair(
        tmp.path(),
        &spec,
        "rust lang",
        200,
        html_body(3).as_bytes(),
        &Ok(results),
    )
    .unwrap();
    let name = body_path.file_name().unwrap().to_str().unwrap().to_string();
    assert!(
        name.starts_with("rust-lang-") && name.ends_with(".html"),
        "{name}"
    );

    let pairs = fixture_pairs(tmp.path(), "fixture").unwrap();
    assert_eq!(pairs.len(), 1);
    let report = run_pair(&spec, &pairs[0]).unwrap();
    assert_eq!(report.outcome, Ok(3));
}

#[test]
fn non_utf8_fixture_body_round_trips() {
    // Live parse decodes with `from_utf8_lossy`, so `--record` can write
    // a fixture whose body is not valid UTF-8 (latin-1 pages); `run_pair`
    // reads bytes and must not hard-fail on them.
    let spec = spec();
    let tmp = tempfile::tempdir().unwrap();
    let mut body = b"<html><body><div class=\"result\"><h2 class=\"t\">T</h2>\
        <a class=\"u\" href=\"/x\">l</a><p class=\"note\">caf"
        .to_vec();
    body.push(0xe9); // latin-1 `é`, invalid UTF-8 -> U+FFFD after lossy decode
    body.extend_from_slice(b" s</p></div></body></html>");

    let results = spec
        .parse_response(200, &body, &base(&spec, "latin1"))
        .unwrap();
    assert_eq!(results[0].snippet, "caf\u{fffd} s");
    write_pair(tmp.path(), &spec, "latin1", 200, &body, &Ok(results)).unwrap();

    let pairs = fixture_pairs(tmp.path(), "fixture").unwrap();
    assert_eq!(pairs.len(), 1);
    let report = run_pair(&spec, &pairs[0]).unwrap();
    assert_eq!(report.outcome, Ok(1));
}

#[test]
fn fixture_pair_detects_drift_and_expected_errors() {
    let spec = spec();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("fixture");
    std::fs::create_dir_all(&dir).unwrap();

    // Drifted expectation -> fail with a diff reason.
    std::fs::write(dir.join("x.html"), html_body(2)).unwrap();
    let expected = ExpectedFixture {
        query: "q".to_string(),
        lang: None,
        page: 1,
        status: 200,
        results: Some(vec![ExpectedResult {
            title: "wrong".to_string(),
            url: "https://x/".to_string(),
            snippet: String::new(),
        }]),
        error: None,
    };
    std::fs::write(
        dir.join("x.expected.json"),
        serde_json::to_string(&expected).unwrap(),
    )
    .unwrap();
    let report = run_pair(&spec, &fixture_pairs(tmp.path(), "fixture").unwrap()[0]).unwrap();
    assert!(report.outcome.is_err());

    // Expected error variant matches a blocked fixture.
    std::fs::write(dir.join("b.html"), b"captcha unusual traffic").unwrap();
    let expected = ExpectedFixture {
        query: "q".to_string(),
        lang: None,
        page: 1,
        status: 200,
        results: None,
        error: Some("blocked".to_string()),
    };
    std::fs::write(
        dir.join("b.expected.json"),
        serde_json::to_string(&expected).unwrap(),
    )
    .unwrap();
    let pairs = fixture_pairs(tmp.path(), "fixture").unwrap();
    let b = pairs.iter().find(|p| p.name == "b").unwrap();
    let report = run_pair(&spec, b).unwrap();
    assert_eq!(report.outcome, Ok(0));
}

#[test]
fn config_dir_spec_overrides_and_resolves() {
    let tmp = tempfile::tempdir().unwrap();
    let engines_dir = tmp.path().join("engines");
    std::fs::create_dir_all(&engines_dir).unwrap();
    std::fs::write(
        engines_dir.join("custom.yaml"),
        SPEC_YAML.replace("id: fixture", "id: custom"),
    )
    .unwrap();

    // load_specs picks the override file up.
    let specs = load_specs(tmp.path(), &env());
    assert!(specs.iter().any(|s| s.id().as_str() == "custom"));

    // resolve_spec_source finds it by entry id (no `spec` field).
    let entry = EngineEntry {
        id: "custom".into(),
        kind: EngineKind::Declarative,
        enabled: true,
        command: None,
        args: vec![],
        cwd: None,
        spec: None,
        tier: None,
        page_size: None,
        egress: None,
        env: BTreeMap::new(),
    };
    let src = resolve_spec_source(&entry, tmp.path()).unwrap();
    assert!(src.contains("id: custom"));

    // ... and by an explicit `spec` path.
    let mut by_path = entry.clone();
    by_path.spec = Some(
        engines_dir
            .join("custom.yaml")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(resolve_spec_source(&by_path, tmp.path()).is_ok());

    // Unknown name -> NotFound.
    let mut missing = entry.clone();
    missing.id = "nope".into();
    assert!(resolve_spec_source(&missing, tmp.path()).is_err());
}

#[test]
fn build_engine_constructs_declarative_from_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let spec_path = tmp.path().join("fixture.yaml");
    std::fs::write(&spec_path, SPEC_YAML).unwrap();
    let entry = EngineEntry {
        id: "fixture".into(),
        kind: EngineKind::Declarative,
        enabled: true,
        command: None,
        args: vec![],
        cwd: None,
        spec: Some(spec_path.to_string_lossy().into_owned()),
        tier: Some(Tier::T3),
        page_size: Some(5),
        egress: None,
        env: BTreeMap::new(),
    };
    let engine = build_engine(&entry, tmp.path()).unwrap();
    assert_eq!(engine.id().as_str(), "fixture");
    assert_eq!(engine.tier(), Tier::T3);
    assert_eq!(engine.page_size(), 5);
}

#[test]
fn build_engines_auto_registers_unconfigured_specs() {
    // Requires OXE_ENGINES unset in the test process (the pin suppresses
    // auto-registration); CI and `mise run test` never set it.
    if std::env::var("OXE_ENGINES").is_ok_and(|v| !v.trim().is_empty()) {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("engines")).unwrap();
    std::fs::write(
        tmp.path().join("engines/auto.yaml"),
        SPEC_YAML.replace("id: fixture", "id: auto"),
    )
    .unwrap();
    let env: EnvMap = BTreeMap::from([(
        "OXE_CONFIG_DIR".to_string(),
        tmp.path().to_string_lossy().into_owned(),
    )]);
    let cfg = Config::from_raw(&toml::Value::Table(toml::Table::new()), &env).unwrap();
    let engines = build_engines(&cfg);
    let auto = engines.iter().find(|e| e.id().as_str() == "auto");
    assert!(
        auto.is_some(),
        "auto spec not registered; got {:?}",
        engines
            .iter()
            .map(|e| e.id().to_string())
            .collect::<Vec<_>>()
    );

    // A `enabled = false` spec is not auto-registered.
    std::fs::write(
        tmp.path().join("engines/off.yaml"),
        SPEC_YAML
            .replace("id: fixture", "id: off")
            .replace("tier: 1", "tier: 3\nenabled: false"),
    )
    .unwrap();
    let engines = build_engines(&cfg);
    assert!(!engines.iter().any(|e| e.id().as_str() == "off"));
}

#[tokio::test]
async fn search_fetches_parses_and_sends_templated_headers() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/s"))
        .and(wiremock::matchers::header("accept-language", "fr"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(html_body(4)))
        .mount(&server)
        .await;

    let yaml = SPEC_YAML.replace(
        "https://search.test.local/s?q={q}&p={page0}&first={offset+1}",
        &format!("{}/s?q={{q}}", server.uri()),
    );
    let spec = CompiledSpec::from_yaml(&yaml, &env()).unwrap();
    let http = HttpClient::from_egress_config(spec.id().clone(), None).unwrap();
    let engine = DeclarativeEngine::new(spec, http);

    let mut r = req("hello world");
    r.lang = Some("fr".to_string());
    let results = engine.search(&r, Duration::from_secs(5)).await.unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(engine.id().as_str(), "fixture");
    assert_eq!(engine.tier(), Tier::T1);
}
