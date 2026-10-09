//! `/app` — the Svelte SPA shell (FX-02/FX-04).
//!
//! `web/src/spa/` is built by `pnpm run build:spa` into
//! `assets/spa/` (index.html plus hashed JS/CSS), which this module embeds.
//! The router mounts the shell at `/app` (exact — the client router renders
//! the in-app root) and `/app/{*rest}` for every client-side route, and
//! redirects the legacy canonical paths (`/`, `/search`, `/answer`) onto
//! their `/app` twins.
//!
//! Serving is a two-step probe: first look for an embedded asset named by
//! the request path (the build's hashed bundles), then fall back to
//! `index.html` so client-side routes boot. `text/css` and
//! `application/javascript` responses carry `Cache-Control: immutable`
//! (Vite emits content-hashed names); everything else is `no-cache`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use rust_embed::Embed;

/// The built SPA bundle, `vite build`'s `outDir` (see `build:spa` in
/// `package.json`; `mise run web` runs it before the freshness diff).
/// Manifest-first: `index.html` plus content-hashed js/css assets.
#[derive(Embed)]
#[folder = "assets/spa/"]
struct SpaAssets;

/// `GET /app` — serve the SPA shell. The client router reads the path for
/// the initial route; an unknown `/app/*` path renders its not-found view.
pub async fn spa() -> impl IntoResponse {
    serve_spa()
}

/// `GET /app/{*rest}` — every SPA route serves the same shell; the hashed
/// assets under `assets/` are served as embedded files. Indexing by the
/// *request path* keeps `/app/index.js`-style direct asset hits working
/// while every page route falls through to `index.html`.
pub async fn spa_nested(uri: Uri) -> impl IntoResponse {
    let asset = uri.path().trim_start_matches('/');
    if let Some(file) = SpaAssets::get(asset) {
        return (
            [(header::CONTENT_TYPE, file.metadata.mimetype().to_string())],
            file.data,
        )
            .into_response();
    }
    serve_spa()
}

/// `GET /`, `/search`, `/answer` — the canonical entry points the old
/// server-rendered pages owned. The SPA is the only UI layer now (FX-06),
/// so each one permanently redirects to its `/app` twin, preserving the
/// query string (`/search?q=x` → `/app/search?q=x`).
pub async fn to_app(uri: Uri) -> Redirect {
    let pq = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    Redirect::permanent(&format!("/app{pq}"))
}

fn serve_spa() -> Response {
    let Some(index) = SpaAssets::get("index.html") else {
        // `cargo build` without a prior `pnpm run build:spa` leaves the
        // embed empty; name the fix in the 503 body.
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "SPA bundle missing — run `pnpm run build:spa` in crates/cauce-server",
        )
            .into_response();
    };
    let body = std::str::from_utf8(index.data.as_ref())
        .unwrap_or("<!doctype html><p>invalid index.html</p>")
        .to_owned();
    Html(body).into_response()
}
