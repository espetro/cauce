//! `/settings` page and the urlencoded `PUT /api/config` form path (W2-07).
//!
//! Acceptance: a page test edits `search.deadline_ms`, saves, reloads and
//! sees the value; the `${env:BIFROST_API_KEY}` template survives a save
//! round-trip byte-for-byte.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

/// Serialises tests that mutate process env: `PUT /api/config` resolves the
/// save path and `${...}` templates against `system_env()`, so the sandbox
/// config dir must be a real env var, not an injected `EnvMap`.
static ENV_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Write `toml_src` to `tmp/cfg/config.toml` and point the process env at
/// the sandbox. Caller must hold `env_lock`.
fn config_env(toml_src: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("config.toml"), toml_src).unwrap();
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", &cfg_dir);
        std::env::set_var("CAUCE_DATA_DIR", tmp.path().join("data"));
    }
    tmp
}

/// Clear the vars `config_env`/`config_env_vars` may set. Caller holds the lock.
fn clear_env() {
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
        std::env::remove_var("CAUCE_DATA_DIR");
        std::env::remove_var("CAUCE_SEARCH_DEADLINE_MS");
        std::env::remove_var("CAUCE_SEARCH_TTL_S");
        std::env::remove_var("CAUCE_ADMISSION_MAX_WAIT_MS");
        std::env::remove_var("CAUCE_ADMISSION_MAX_CONCURRENT_PER_ENGINE");
        std::env::remove_var("CAUCE_LOGS_RETENTION_DAYS");
        std::env::remove_var("CAUCE_AI_BASE_URL");
        std::env::remove_var("CAUCE_AI_API_KEY");
        std::env::remove_var("CAUCE_AI_MODEL");
        std::env::remove_var("CAUCE_AI_ENABLED");
        std::env::remove_var("CAUCE_ENGINES");
        std::env::remove_var("BIFROST_API_KEY");
    }
}

/// A replay-engine app whose `Config` was `load()`ed against the sandbox env.
fn app(tmp: &tempfile::TempDir) -> Router {
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, Config::load().expect("config"));
    build_router(state)
}

async fn call(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    call(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "text/html")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// `PUT /api/config` with an urlencoded form body, as htmx sends it.
async fn put_form(router: &Router, body: &str, hx: bool) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(Method::PUT)
        .uri("/api/config")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("X-Cauce-Client", "ui");
    if hx {
        req = req.header("HX-Request", "true");
    }
    call(router, req.body(Body::from(body.to_string())).unwrap()).await
}

fn saved_config(tmp: &tempfile::TempDir) -> String {
    std::fs::read_to_string(tmp.path().join("cfg/config.toml")).expect("config file")
}

#[tokio::test]
async fn settings_page_renders_sections_and_request_id() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for needle in [
        "<legend>Search</legend>",
        "<legend>Engines</legend>",
        "<legend>Admission</legend>",
        "<legend>Logging</legend>",
        "AI answers",
        "hx-put=\"/api/config\"",
        "name=\"search.deadline_ms\"",
        "name=\"search.ttl_s\"",
        "name=\"admission.max_wait_ms\"",
        "name=\"logs.retention_days\"",
        "name=\"ai.base_url\"",
        "name=\"ai.api_key\"",
        "name=\"ai.model\"",
        "engines.replay.enabled",
        "engines.ddgs.enabled",
        "engines.replay.egress.proxy",
        "list=\"ai-models\"",
        "role=\"status\" aria-live=\"polite\"",
        "<code class=\"request-id\">",
        // W3-01 hedge knobs are real editable fields now.
        "name=\"search.min_results\"",
        "name=\"search.hedge_floor_ms\"",
        "name=\"search.hedge_ceiling_ms\"",
        // Cross-links and the cache block.
        "href=\"/engines\"",
        "id=\"cache-block\"",
        "hx-delete=\"/api/cache?expired=true\"",
        "hx-delete=\"/api/cache?all=true\"",
        "href=\"/cache\"",
    ] {
        assert!(body.contains(needle), "settings page missing {needle:?}");
    }
    // The delete buttons live inside the settings form: without
    // `hx-params="none"` htmx appends every enabled field to the DELETE
    // URL and `DELETE /api/cache` 400s on the unknown params.
    assert_eq!(
        body.matches("hx-params=\"none\"").count(),
        2,
        "both cache delete buttons must opt out of form params: {body}"
    );
    assert!(
        !body.contains("hx-ext="),
        "the form PUTs urlencoded fields; json-enc is not loaded here: {body}"
    );
    clear_env();
}

/// Acceptance: edit `search.deadline_ms` on the page, save, reload, see it.
#[tokio::test]
async fn deadline_edit_saves_and_reloads() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[search]\ndeadline_ms = 3000\n");
    let app = app(&tmp);

    let (status, body) = put_form(&app, "search.deadline_ms=1234", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json response");
    assert_eq!(json["search"]["deadline_ms"], 1234);
    assert_eq!(json["effective_after_restart"], true);

    let on_disk = saved_config(&tmp);
    assert!(
        on_disk.contains("deadline_ms = 1234"),
        "file should carry the new deadline: {on_disk}"
    );

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("name=\"search.deadline_ms\" value=\"1234\""),
        "reloaded page should show the saved deadline: {body}"
    );
    clear_env();
}

/// Acceptance: the `${env:BIFROST_API_KEY}` template is shown verbatim and
/// survives a save round-trip byte-for-byte in `config.toml`.
#[tokio::test]
async fn api_key_template_survives_roundtrip() {
    let _guard = env_lock().await;
    clear_env();
    // SAFETY: serialized by ENV_LOCK; PUT validation resolves `${env:...}`
    // against the process env.
    unsafe { std::env::set_var("BIFROST_API_KEY", "sk-test-key") };

    let raw = "[ai]\nbase_url = \"\"\napi_key = \"${env:BIFROST_API_KEY}\"\n";
    let tmp = config_env(raw);
    let app = app(&tmp);

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("value=\"${env:BIFROST_API_KEY}\""),
        "api_key input must show the template verbatim: {body}"
    );
    assert!(
        body.contains("BIFROST_API_KEY is set"),
        "env status should report the var as set: {body}"
    );

    // Save the whole form (the field value is the raw template text).
    let (status, body) = put_form(
        &app,
        "search.deadline_ms=3000&ai.api_key=${env:BIFROST_API_KEY}",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let on_disk = saved_config(&tmp);
    assert!(
        on_disk.contains("api_key = \"${env:BIFROST_API_KEY}\""),
        "template must survive byte-for-byte: {on_disk}"
    );
    assert!(
        !on_disk.contains("sk-test-key"),
        "the resolved secret must never reach the file: {on_disk}"
    );

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("value=\"${env:BIFROST_API_KEY}\""),
        "reloaded page must still show the template: {body}"
    );
    clear_env();
}

#[tokio::test]
async fn invalid_field_reports_inline_error() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);

    // Plain form submit (no HX header): the JSON envelope still applies.
    let (status, body) = put_form(&app, "search.deadline_ms=soon", false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json envelope");
    assert_eq!(json["error"]["code"], "invalid_config");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("deadline_ms")
    );

    // The htmx submit swaps the error fragment into the page (200 + inline):
    // a status line plus an out-of-band error under the offending input.
    let (status, body) = put_form(&app, "search.deadline_ms=soon", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("form-status error"), "{body}");
    assert!(body.contains("not saved: 1 error"), "{body}");
    assert!(
        body.contains("id=\"fe-search-ddeadline_ms\""),
        "error line targets the deadline field: {body}"
    );
    let pos = body.find("id=\"fe-search-ddeadline_ms\"").unwrap();
    assert!(
        body[pos..].contains("hx-swap-oob"),
        "error element swaps out of band: {body}"
    );
    clear_env();
}

#[tokio::test]
async fn engine_fields_write_file_entries() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = put_form(
        &app,
        "engines.replay.enabled=true&engines.replay.tier=2&engines.replay.egress.proxy=http%3A%2F%2F127.0.0.1%3A8888",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let on_disk = saved_config(&tmp);
    let tree = toml::from_str::<toml::Value>(&on_disk).expect("saved TOML parses");
    let replay = tree["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some("replay"))
        .expect("replay entry written");
    assert_eq!(replay["enabled"].as_bool(), Some(true));
    assert_eq!(replay["tier"].as_integer(), Some(2));
    assert_eq!(
        replay["egress"]["proxy"].as_str(),
        Some("http://127.0.0.1:8888")
    );
    clear_env();
}

/// Engine field errors and clears target the row's `fe-engines-<id>`
/// element — the page renders one error line per row, never a per-field
/// `fe-engines-<id>-<field>` phantom.
#[tokio::test]
async fn engine_field_errors_target_the_row() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);

    let (status, body) = put_form(&app, "engines.replay.tier=9", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("not saved: 1 error"), "{body}");
    assert!(
        body.contains("id=\"fe-engines-dreplay\""),
        "the row error element must carry the message: {body}"
    );
    assert!(
        !body.contains("fe-engines-dreplay-dtier"),
        "no per-field error element exists: {body}"
    );

    // A clean save clears the row element exactly once even though several
    // `engines.replay.*` fields were submitted.
    let (status, body) = put_form(
        &app,
        "engines.replay.tier=2&engines.replay.egress.proxy=http%3A%2F%2F127.0.0.1%3A8888&search.deadline_ms=1234",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body.matches("id=\"fe-engines-dreplay\"").count(),
        1,
        "one OOB clear for the row: {body}"
    );
    assert!(!body.contains("fe-engines-dreplay-dtier"), "{body}");
    assert!(
        !body.contains("fe-engines-dreplay-degress-dproxy"),
        "{body}"
    );
    assert!(body.contains("id=\"fe-search-ddeadline_ms\""), "{body}");
    clear_env();
}

/// htmx resolves oob targets with `querySelector("#<id>")`, where a dot
/// parses as a class selector and the swap silently misses. Every `fe-*`
/// id the page renders and the status fragment emits must be dot-free, and
/// the emitted id must be the exact id of an element on the page so
/// `document.getElementById` finds it — checked here with a dotted engine
/// id, the case that produced dotted row ids.
#[tokio::test]
async fn oob_error_ids_are_dot_free_and_match_the_page() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[[engines]]\nid = \"dotted.id\"\nkind = \"replay\"\n");
    let app = app(&tmp);

    // The page's row error element carries the encoded id.
    let (status, page) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("id=\"fe-engines-ddotted-did\""),
        "the dotted-id row must render a dot-free error element: {page}"
    );
    for id in fe_ids(&page) {
        assert!(!id.contains('.'), "page fe-* id holds a dot: {id:?}");
    }

    // The oob fragment for a bad engine field emits the very same id, so a
    // JS-side `document.getElementById` lands on the rendered row.
    let (status, body) = put_form(&app, "engines.dotted.id.tier=9", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("id=\"fe-engines-ddotted-did\""),
        "oob error must target the rendered row id: {body}"
    );
    for id in fe_ids(&body) {
        assert!(!id.contains('.'), "oob fe-* id holds a dot: {id:?}");
    }
    clear_env();
}

/// `a.b` and `a-b` are both legal engine ids (`[A-Za-z0-9._-]+`). They
/// used to share `fe-engines-a-b` — a duplicate element id that merged
/// the rows' error lines. The injective encoding gives each row its own.
#[tokio::test]
async fn dotted_and_dashed_engine_ids_get_distinct_row_ids() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env(
        "[[engines]]\nid = \"a.b\"\nkind = \"replay\"\n\n[[engines]]\nid = \"a-b\"\nkind = \"replay\"\n",
    );
    let app = app(&tmp);
    let (status, page) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("id=\"fe-engines-da-db\""), "{page}");
    assert!(page.contains("id=\"fe-engines-da--b\""), "{page}");
    assert!(
        !page.contains("id=\"fe-engines-a-b\""),
        "the old colliding id must be gone: {page}"
    );
    // No `fe-*` id appears twice anywhere on the page.
    let mut ids = fe_ids(&page);
    ids.sort();
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "duplicate fe-* ids: {ids:?}");
    clear_env();
}

/// An id outside `[A-Za-z0-9._-]+` is rejected at config parse, so a TOML
/// `PUT /api/config` carrying one fails in-memory validation (400, error
/// naming the id and the allowed charset) and never reaches the file.
#[tokio::test]
async fn invalid_engine_id_charset_is_rejected() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::PUT)
            .uri("/api/config")
            .header("Content-Type", "application/toml")
            .body(Body::from(
                "[[engines]]\nid = \"a b\"\nkind = \"replay\"\n".to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json envelope");
    assert_eq!(json["error"]["code"], "invalid_config");
    let msg = json["error"]["message"].as_str().unwrap();
    assert!(msg.contains("a b"), "{msg}");
    assert!(msg.contains("[A-Za-z0-9._-]+"), "{msg}");
    assert_eq!(
        saved_config(&tmp),
        "",
        "a rejected config must never be written"
    );
    clear_env();
}

/// Every `id="fe-..."` value in `html`, for the dot-free assertions.
fn fe_ids(html: &str) -> Vec<String> {
    html.split("id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('\"').next())
        .filter(|id| id.starts_with("fe-"))
        .map(String::from)
        .collect()
}

#[tokio::test]
async fn model_picker_lists_provider_models() {
    let _guard = env_lock().await;
    clear_env();
    // Stand-in for `GET {base_url}/models` (OpenAI listing shape).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/models",
                axum::routing::get(|headers: axum::http::HeaderMap| async move {
                    assert_eq!(
                        headers.get(axum::http::header::AUTHORIZATION),
                        Some(&axum::http::HeaderValue::from_static(
                            "Bearer settings-test-token"
                        ))
                    );
                    axum::Json(serde_json::json!({
                        "data": [{"id": "alpha-1"}, {"id": "beta-2"}]
                    }))
                }),
            ),
        )
        .await
        .unwrap();
    });

    let raw = format!(
        "[ai]\nbase_url = \"http://{addr}/v1\"\napi_key = \"settings-test-token\"\nmodel = \"alpha-1\"\n"
    );
    let tmp = config_env(&raw);
    let app = app(&tmp);
    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("<option value=\"alpha-1\" />"),
        "datalist should carry the provider models: {body}"
    );
    assert!(
        body.contains("<option value=\"beta-2\" />"),
        "datalist should carry the provider models: {body}"
    );
    assert!(
        body.contains("name=\"ai.model\" value=\"alpha-1\""),
        "current model should prefill the picker: {body}"
    );
    clear_env();
}

#[tokio::test]
async fn model_picker_falls_back_to_free_text() {
    let _guard = env_lock().await;
    clear_env();
    // Nothing listens on this port: the listing fails and the input degrades
    // to free text.
    let tmp = config_env("[ai]\nbase_url = \"http://127.0.0.1:1/v1\"\n");
    let app = app(&tmp);
    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("name=\"ai.model\""),
        "free-text model input must still render: {body}"
    );
    assert!(body.contains("model list unreachable"), "{body}");
    clear_env();
}

/// A `CAUCE_*`-pinned field renders disabled, so a save cannot bake the env
/// value into the file.
#[tokio::test]
async fn env_overridden_field_is_disabled() {
    let _guard = env_lock().await;
    clear_env();
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe { std::env::set_var("CAUCE_SEARCH_DEADLINE_MS", "9999") };
    let tmp = config_env("[search]\ndeadline_ms = 3000\n");
    let app = app(&tmp);

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pos = body
        .find("name=\"search.deadline_ms\"")
        .expect("deadline input");
    assert!(
        body[..pos + 200].contains("disabled"),
        "env-overridden input must be disabled"
    );
    assert!(body.contains("set by CAUCE_SEARCH_DEADLINE_MS"), "{body}");

    // A save of another field leaves the file's deadline untouched.
    let (status, body) = put_form(&app, "search.ttl_s=10", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let on_disk = saved_config(&tmp);
    assert!(
        on_disk.contains("deadline_ms = 3000"),
        "env override must not be baked into the file: {on_disk}"
    );
    clear_env();
}

/// Multiple invalid fields each get their own error line and the status
/// counts them.
#[tokio::test]
async fn multiple_invalid_fields_report_per_field_errors() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);

    let (status, body) = put_form(
        &app,
        "search.deadline_ms=soon&admission.max_wait_ms=later",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("not saved: 2 errors"), "{body}");
    for id in ["fe-search-ddeadline_ms", "fe-admission-dmax_wait_ms"] {
        assert!(body.contains(&format!("id=\"{id}\"")), "{body}");
    }
    // The file is untouched: values stay in the submitted form only.
    assert_eq!(saved_config(&tmp), "");
    clear_env();
}

/// A successful htmx save reports `saved HH:MM` and clears field errors.
#[tokio::test]
async fn successful_save_reports_saved_time() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = put_form(&app, "search.deadline_ms=1234", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("form-status ok"), "{body}");
    let pos = body.find("saved ").expect("status text");
    let hhmm = &body[pos + 6..pos + 11];
    assert!(
        hhmm.chars().nth(2) == Some(':')
            && hhmm.replace(':', "").chars().all(|c| c.is_ascii_digit()),
        "status should carry a HH:MM save time: {body}"
    );
    clear_env();
}

/// `config.put` audit rows name the changed keys and the `ui` actor.
#[tokio::test]
async fn save_writes_audit_with_changed_keys() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[search]\ndeadline_ms = 3000\n");
    let app = app(&tmp);

    let (status, body) = put_form(&app, "search.deadline_ms=1234", true).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/api/audit?action=config.put")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Value = serde_json::from_str(&body).expect("audit rows");
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"].as_str() == Some("config.put"))
        .expect("config.put audit row");
    assert_eq!(row["actor"].as_str(), Some("ui"), "{row}");
    assert_eq!(
        row["details"]["changed"],
        serde_json::json!(["search.deadline_ms"]),
        "{row}"
    );
    clear_env();
}

/// A file-literal secret renders as typed on the page (the owner's own
/// config file); the `restore_redacted` pass still keeps a `<redacted>`
/// submission from clobbering the secret, so the audit row must not report
/// `ai.api_key` as changed.
#[tokio::test]
async fn literal_secret_renders_and_redacted_restore_holds() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("[ai]\napi_key = \"sk-file-literal\"\n");
    let app = app(&tmp);

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("name=\"ai.api_key\" value=\"sk-file-literal\""),
        "a literal secret must render as typed: {body}"
    );
    assert!(
        !body.contains("&#60;redacted&#62;"),
        "the <redacted> placeholder must not reach the input: {body}"
    );

    // A stale page (or a JSON API client) can still submit the decoded
    // `<redacted>` placeholder verbatim; the restore pass writes the real
    // secret back rather than persisting the literal.
    let (status, body) = put_form(&app, "ai.api_key=%3Credacted%3E", false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        saved_config(&tmp).contains("api_key = \"sk-file-literal\""),
        "the restore must write the file secret back"
    );

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/api/audit?action=config.put")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Value = serde_json::from_str(&body).expect("audit rows");
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"].as_str() == Some("config.put"))
        .expect("config.put audit row");
    assert_eq!(
        row["details"]["changed"],
        serde_json::json!([]),
        "a restored redacted leaf is no change: {row}"
    );
    clear_env();
}

/// Under `CAUCE_ENGINES=replay` the `enabled` checkboxes render as plain
/// text (`enabled` / `disabled`) — no `engines.<id>.enabled` input exists
/// at all, so a browser submits no enabled pair and a full-form save
/// creates the tier stanza without baking the pinned flag into the file.
#[tokio::test]
async fn pinned_engine_tier_edit_writes_no_enabled() {
    let _guard = env_lock().await;
    clear_env();
    // SAFETY: serialized by ENV_LOCK.
    unsafe { std::env::set_var("CAUCE_ENGINES", "replay") };
    let tmp = config_env("");
    let app = app(&tmp);

    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !body.contains(".enabled\""),
        "pinned engine rows render no submittable enabled field: {body}"
    );
    for needle in [
        "<span class=\"engine-enabled\">enabled</span>",
        "<span class=\"engine-enabled\">disabled</span>",
        "enabled flags are pinned by CAUCE_ENGINES",
    ] {
        assert!(body.contains(needle), "pinned row text missing {needle:?}");
    }

    // The full shape the pinned form submits: every rendered field except
    // the disabled `enabled` pairs.
    let (status, body) = put_form(
        &app,
        concat!(
            "search.deadline_ms=5000&search.ttl_s=60",
            "&engines.replay.tier=2&engines.replay.egress.proxy=",
            "&engines.ddgs.tier=&engines.ddgs.egress.proxy=",
            "&admission.max_wait_ms=250&admission.max_concurrent_per_engine=4",
            "&logs.retention_days=14&ai.base_url=&ai.api_key=&ai.model=",
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let on_disk = saved_config(&tmp);
    let tree = toml::from_str::<toml::Value>(&on_disk).expect("saved TOML parses");
    let replay = tree["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some("replay"))
        .expect("replay stanza written");
    assert_eq!(replay["tier"].as_integer(), Some(2));
    assert!(
        !on_disk.contains("enabled"),
        "the CAUCE_ENGINES pin must not persist anywhere: {on_disk}"
    );
    assert!(
        replay.get("env").is_none(),
        "env must never serialise: {on_disk}"
    );
    clear_env();
}

/// `CAUCE_AI_ENABLED` pins the AI checkbox copy to `set by CAUCE_AI_ENABLED`.
#[tokio::test]
async fn ai_enabled_env_pin_shows_hint() {
    let _guard = env_lock().await;
    clear_env();
    // SAFETY: serialized by ENV_LOCK.
    unsafe { std::env::set_var("CAUCE_AI_ENABLED", "false") };
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = get_html(&app, "/settings").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("set by CAUCE_AI_ENABLED"), "{body}");
    clear_env();
}

/// `GET /settings?fragment=cache` returns the cache fieldset alone for the
/// in-place refresh after `delete expired` / `delete all`.
#[tokio::test]
async fn cache_fragment_returns_block_only() {
    let _guard = env_lock().await;
    clear_env();
    let tmp = config_env("");
    let app = app(&tmp);
    let (status, body) = get_html(&app, "/settings?fragment=cache").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("id=\"cache-block\""), "{body}");
    assert!(body.contains("entries"), "{body}");
    assert!(body.contains("unexpired"), "{body}");
    assert_eq!(
        body.matches("hx-params=\"none\"").count(),
        2,
        "the refreshed block keeps the delete buttons' hx-params opt-out: {body}"
    );
    assert!(
        !body.contains("<legend>Search</legend>"),
        "fragment must not carry the full page: {body}"
    );
    clear_env();
}
