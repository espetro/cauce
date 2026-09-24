//! `/api/config` handlers: the redacted effective-config read and the
//! one-write-path `PUT` shared by the TOML body and the `/settings` form.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::config::{Config, system_env};
use serde_json::{Value, json};

use super::{changed_config_paths, write_audit};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// `GET /api/config`: the effective config, redacted. `Config`'s `Serialize`
/// impl renders `${env:...}`/`${file:...}` templates literally, so resolved
/// secrets can never appear in the response.
pub async fn config_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> Result<Json<Value>, ApiError> {
    state
        .with_config(|cfg| serde_json::to_value(cfg))
        .map(Json)
        .map_err(|e| ctx.err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))
}

/// `PUT /api/config`: replace the config file with the submitted tree.
///
/// Two body encodings are accepted: the canonical TOML document and, for
/// the `/settings` page (W2-07), `application/x-www-form-urlencoded` fields
/// named after dotted config paths that [`crate::settings`] merges onto the
/// raw file tree. Both encodings share the rest of the pipeline below, so
/// there is one write path.
///
/// Validation runs in-memory against the current process environment *before*
/// any write, so a crash or `kill -9` cannot leave `config.toml` in an
/// unbootable state. On success the new raw tree is written atomically and
/// `state.config` is swapped; the running pipeline/engines still use the
/// values they were started with, so the response carries
/// `effective_after_restart: true`.
///
/// A form submit marked `HX-Request` gets an HTML fragment back (200 on both
/// success and validation failure, so htmx swaps it inline); everything else
/// gets the JSON body / envelope.
pub async fn config_put(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let form = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    let result = config_put_inner(&state, &ctx, &headers, &body, form).await;
    #[cfg(feature = "ui")]
    if form && headers.get("hx-request").is_some() {
        // On a failed save, re-run the fields one by one so the fragment can
        // place an error line under each offending input.
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(std::str::from_utf8(&body).unwrap_or("").as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
        let submitted: Vec<String> = pairs.iter().map(|(k, _)| k.clone()).collect();
        let (engine_ids, field_errors) = state.with_config(|cfg| {
            let ids = cfg
                .engines
                .iter()
                .map(|e| e.id.to_string())
                .collect::<Vec<_>>();
            let errs = if result.is_err() {
                crate::settings::field_errors(cfg, &pairs)
            } else {
                Vec::new()
            };
            (ids, errs)
        });
        return Ok(crate::html::settings_status(
            result,
            field_errors,
            submitted,
            &engine_ids,
        ));
    }
    result.map(Json::into_response)
}

async fn config_put_inner(
    state: &AppState,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    body: &Bytes,
    form: bool,
) -> Result<Json<Value>, ApiError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| ctx.bad_request("PUT /api/config expects a UTF-8 TOML or form body"))?;
    // Snapshot the file layer before the merge so the audit row can name the
    // keys that actually changed (`{"changed": ["search.deadline_ms"]}`).
    let old_tree = state.with_config(|cfg| {
        cfg.raw_tree()
            .cloned()
            .or_else(|| cfg.display_tree().ok())
            .unwrap_or_else(|| toml::Value::Table(toml::Table::new()))
    });
    let mut tree = if form {
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(text.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        state
            .with_config(|cfg| crate::settings::merge_form_config(cfg, &pairs))
            .map_err(|e| {
                ctx.err(
                    StatusCode::BAD_REQUEST,
                    "invalid_config",
                    format!("invalid config: {e}"),
                )
            })?
    } else {
        toml::from_str::<toml::Value>(text).map_err(|e| {
            ctx.err(
                StatusCode::BAD_REQUEST,
                "invalid_config",
                format!("TOML: {e}"),
            )
        })?
    };
    // `<redacted>` leaves from a `GET /api/config` roundtrip get their real
    // values back from the current config; a literal `<redacted>` with no
    // current secret behind it is rejected rather than persisted.
    let restored = state
        .with_config(|cfg| cfg.restore_redacted(&mut tree))
        .map_err(|e| {
            ctx.err(
                StatusCode::BAD_REQUEST,
                "invalid_config",
                format!("invalid config: {e}"),
            )
        })?;
    if !restored.is_empty() {
        tracing::info!(paths = ?restored, "restored redacted config secrets on PUT");
    }

    // Computed after `restore_redacted`: a `<redacted>` leaf the restore
    // just filled back with the file's own secret is no change at all.
    let changed = changed_config_paths(&old_tree, &tree);

    // In-memory validation: resolve `${...}` templates, apply `CAUCE_*` env
    // overrides, check schema and engine pinning. The original file is not
    // touched until we know the candidate is loadable.
    let new_cfg = Config::from_raw(&tree, &system_env()).map_err(|e| {
        ctx.err(
            StatusCode::BAD_REQUEST,
            "invalid_config",
            format!("invalid config: {e}"),
        )
    })?;

    // All sync file IO inside the lock; nothing awaits in the closure.
    let loaded = state.with_config(|cfg| {
        if let Err(e) = new_cfg.save() {
            return Err(ctx.err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("cannot write {}: {e}", new_cfg.config_path().display()),
            ));
        }
        *cfg = new_cfg.clone();
        Ok(new_cfg)
    })?;

    write_audit(
        state.store(),
        ctx,
        headers,
        "config.put",
        loaded.config_path().display().to_string(),
        json!({"changed": changed}),
    )
    .await?;
    let mut body = serde_json::to_value(&loaded)
        .map_err(|e| ctx.err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?;
    if let Value::Object(m) = &mut body {
        m.insert("effective_after_restart".to_string(), json!(true));
    }
    Ok(Json(body))
}
