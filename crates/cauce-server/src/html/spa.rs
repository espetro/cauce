//! The Svelte 5 SPA shell and its emitted assets — `GET /app` and
//! `GET /app/{*rest}` (FX-02, plan `.agents/plans/2026-09-28-frontend-
//! replacement.md` §5.1). `vite build` emits `web/src/spa` into
//! `assets/spa/` (committed like `assets/app.js`); the files below are
//! embedded at compile time via `rust-embed`, so a build without the
//! `ui` feature carries no SPA surface at all.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::error::ApiError;
use axum::extract::Path;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

/// The vite output tree (`index.html` + hashed `assets/*`), embedded.
#[derive(Embed)]
#[folder = "assets/spa/"]
struct SpaAssets;

/// `GET /app`: the SPA shell. Every other `/app/*` path either names an
/// emitted asset or is a client-side route the shell bootstraps.
pub async fn spa() -> Response {
    serve_spa("index.html")
}

/// `GET /app/{*rest}`: serves the embedded asset `rest` names, else the
/// `index.html` fallback so client-side routes survive refresh and deep
/// links (`/app/search`, `/app/answer/…`).
pub async fn spa_nested(Path(rest): Path<String>) -> Response {
    serve_spa(&rest)
}

fn serve_spa(name: &str) -> Response {
    match SpaAssets::get(name).or_else(|| SpaAssets::get("index.html")) {
        Some(file) => (
            [
                (header::CONTENT_TYPE, file.metadata.mimetype()),
                (header::CACHE_CONTROL, cache_control(name)),
            ],
            file.data,
        )
            .into_response(),
        // A ui build without `assets/spa/` is a toolchain bug, not a
        // runtime state — the `web` gate's freshness check is what keeps
        // the committed bundle present.
        None => ApiError::not_found("spa bundle not embedded").into_response(),
    }
}

/// Hashed assets (`assets/index-<hash>.js`) are content-addressed, so
/// they cache forever; the shell itself must revalidate to pick up new
/// binary builds.
fn cache_control(name: &str) -> &'static str {
    if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}
