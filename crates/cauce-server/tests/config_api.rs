//! `GET`/`PUT /api/config` tests (W0-09): `${env:...}` template
//! redaction, the put round-trip + validation failures, and the
//! `<redacted>` placeholder restore.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cauce_core::config::Config;
use cauce_server::build_router;

mod support;
use support::*;

/// `GET /api/config` shows `${env:...}` templates literally and never a
/// resolved secret; `PUT /api/config` round-trips a valid change, rejects
/// an invalid one with a 400, and audits the write.
#[tokio::test]
async fn config_get_redaction_and_put_roundtrip() {
    let _guard = env_lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&config_dir).unwrap();
    // SAFETY: serialized by env_lock; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", &config_dir);
        std::env::set_var("CAUCE_DATA_DIR", tmp.path().join("data"));
        std::env::set_var("CAUCE_ROUTE_TEST_SECRET", "s3cret-value");
        // The env overlay would otherwise beat the PUT body's values.
        std::env::remove_var("CAUCE_AI_API_KEY");
        std::env::remove_var("CAUCE_SEARCH_DEADLINE_MS");
    }
    std::fs::write(
        config_dir.join("config.toml"),
        "[ai]\napi_key = \"${env:CAUCE_ROUTE_TEST_SECRET}\"\n",
    )
    .unwrap();

    let (state, _tmp2) = test_state();
    // Point the state's config at the sandbox (Config::load reads env).
    state.with_config(|cfg| *cfg = Config::load().unwrap());
    assert_eq!(state.with_config(|c| c.ai.api_key.clone()), "s3cret-value");
    let router = build_router(state);

    let (status, _, body) = get(&router, "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    let text = body.to_string();
    assert!(
        text.contains("${env:CAUCE_ROUTE_TEST_SECRET}"),
        "template must be shown literally: {text}"
    );
    assert!(
        !text.contains("s3cret-value"),
        "resolved secret must never appear: {text}"
    );

    // PUT a valid tree: the value lands, templates elsewhere stay literal,
    // the write is audited.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .header("content-type", "application/toml")
        .body(Body::from("[search]\ndeadline_ms = 1234\n"))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["search"]["deadline_ms"], 1234);
    // The PUT body is the whole new config; `ai.api_key` falls back to its
    // blank default now that the file's `${env:...}` template is gone.
    assert_eq!(body["ai"]["api_key"], "");
    assert_eq!(
        Config::load().unwrap().search.deadline_ms,
        1234,
        "the file must carry the new tree"
    );

    let (_, _, body) = get(&router, "/api/audit").await;
    assert!(
        body.as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "config.put" && r["actor"] == "api"),
        "audit rows: {body}"
    );

    // PUT an invalid tree (unknown key): 400 and the file is restored.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from("bogus_key = 1\n"))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "invalid_config");
    assert_eq!(Config::load().unwrap().search.deadline_ms, 1234);

    // PUT a tree whose template cannot resolve: 400, file restored.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from(
            "[ai]\napi_key = \"${env:CAUCE_UNSET_FOR_TEST}\"\n",
        ))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "invalid_config");
    assert_eq!(Config::load().unwrap().search.deadline_ms, 1234);

    // Malformed TOML: 400, no file touched.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from("[unclosed"))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "invalid_config");
}

/// #82: a secret injected via an `CAUCE_*` env override — no `${env:...}`
/// template in the file — must still never reach `GET /api/config`.
#[tokio::test]
async fn config_get_redacts_env_override_secret() {
    let _guard = env_lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&config_dir).unwrap();
    // SAFETY: serialized by env_lock; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", &config_dir);
        std::env::set_var("CAUCE_DATA_DIR", tmp.path().join("data"));
        std::env::set_var("CAUCE_AI_API_KEY", "env-override-secret");
    }

    let (state, _tmp2) = test_state();
    state.with_config(|cfg| *cfg = Config::load().unwrap());
    let router = build_router(state);

    let (status, _, body) = get(&router, "/api/config").await;
    unsafe {
        std::env::remove_var("CAUCE_AI_API_KEY");
    }
    assert_eq!(status, StatusCode::OK);
    let text = body.to_string();
    assert!(
        !text.contains("env-override-secret"),
        "env-override secret must never appear: {text}"
    );
    assert_eq!(body["ai"]["api_key"], "<redacted>");
}

/// A `GET` -> edit -> `PUT` roundtrip restores `<redacted>` placeholders
/// from the current config instead of writing the literal into
/// `config.toml`; a placeholder with no current secret is a 400.
#[tokio::test]
async fn config_put_restores_redacted_placeholders() {
    let _guard = env_lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&config_dir).unwrap();
    // SAFETY: serialized by env_lock; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", &config_dir);
        std::env::set_var("CAUCE_DATA_DIR", tmp.path().join("data"));
        std::env::set_var("CAUCE_AI_API_KEY", "env-override-secret");
    }

    let (state, _tmp2) = test_state();
    state.with_config(|cfg| *cfg = Config::load().unwrap());
    let router = build_router(state);

    // PUT the redacted display shape back: the placeholder is restored.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .header("content-type", "application/toml")
        .body(Body::from("[ai]\napi_key = \"<redacted>\"\n"))
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let file = std::fs::read_to_string(config_dir.join("config.toml")).unwrap();
    assert!(
        file.contains("env-override-secret"),
        "restored secret must land in the file: {file}"
    );
    assert!(
        !file.contains("<redacted>"),
        "placeholder must never persist: {file}"
    );

    // A placeholder with nothing behind it is rejected.
    let bad = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .header("content-type", "application/toml")
        .body(Body::from("[search]\ndeadline_ms = \"<redacted>\"\n"))
        .unwrap();
    let (status, _, body) = call_json(&router, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "invalid_config");

    unsafe {
        std::env::remove_var("CAUCE_AI_API_KEY");
    }
}
