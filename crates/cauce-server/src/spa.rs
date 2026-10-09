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
//! the wildcard tail (the build's hashed bundles live at `assets/`),
//! then fall back to `index.html` so client-side routes boot. `assets/*`
//! responses carry `Cache-Control: immutable` (Vite emits content-hashed
//! names); everything else is `no-cache`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::extract::Path;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
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
    serve_spa("index.html")
}

/// `GET /app/{*rest}` — every SPA route serves the same shell; the hashed
/// assets under `assets/` are served as embedded files. The `{*rest}`
/// wildcard binds the tail *after* `/app`, so `/app/assets/index-<hash>.js`
/// probes `assets/index-<hash>.js` in the embed — the full `uri.path()`
/// includes the mount prefix and never matches.
pub async fn spa_nested(Path(rest): Path<String>) -> impl IntoResponse {
    serve_spa(&rest)
}

/// `GET /`, `/search`, `/answer` — the canonical entry points the old
/// server-rendered pages owned. The SPA is the only UI layer now (FX-06),
/// so each one permanently redirects to its `/app` twin, preserving the
/// query string (`/search?q=x` → `/app/search?q=x`).
pub async fn to_app(uri: Uri) -> Redirect {
    let pq = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let (path, query) = match pq.split_once('?') {
        Some((p, q)) => (p, format!("?{q}")),
        None => (pq, String::new()),
    };
    // `/` must land on `/app`, not `/app/`: the router mounts `/app`
    // exactly and `/app/{*rest}` needs a non-empty tail, so a literal
    // `/app/` 404s.
    Redirect::permanent(&format!("/app{}{}", path.trim_end_matches('/'), query))
}

/// `GET /app/` — the wildcard tail only matches when non-empty, so
/// hand-typed trailing slashes permanently redirect to `/app`.
pub async fn app_root() -> Redirect {
    Redirect::permanent("/app")
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
        // `cargo build` without a prior `pnpm run build:spa` leaves the
        // embed empty; name the fix in the 503 body.
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SPA bundle missing — run `pnpm run build:spa` in crates/cauce-server",
        )
            .into_response(),
    }
}

/// Hashed assets (`assets/index-<hash>.js`) are content-addressed, so they
/// cache forever; the shell itself must revalidate to pick up new binary
/// builds.
fn cache_control(name: &str) -> &'static str {
    if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}
