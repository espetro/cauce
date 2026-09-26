//! Static assets vendored under `crates/cauce-server/assets` (embedded at
//! compile time via `rust-embed`) and the small document endpoints that
//! serve them — `GET /favicon.ico` and `GET /opensearch.xml`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::borrow::Cow;
use std::sync::LazyLock;

use axum::Extension;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

use crate::app::{AppState, RouterOptions};

/// Static assets vendored under `crates/cauce-server/assets`.
#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

fn asset_string(name: &str) -> String {
    Assets::get(name)
        .map(|f| String::from_utf8_lossy(&f.data).into_owned())
        .unwrap_or_default()
}

pub(crate) static HTMX_JS: LazyLock<String> = LazyLock::new(|| asset_string("htmx.min.js"));
pub(crate) static JSON_ENC_JS: LazyLock<String> = LazyLock::new(|| asset_string("json-enc.js"));
/// Shared page stylesheet; the other `ui` pages (W2) inject it too.
pub(crate) static STYLE_CSS: LazyLock<String> = LazyLock::new(|| asset_string("style.css"));

/// The version label the shared header renders (`v0.0.0`), hidden below
/// 640 px by the stylesheet. Referenced from `templates/header.html`.
pub(crate) const VERSION_LABEL: &str = concat!("v", env!("CARGO_PKG_VERSION"));

static FAVICON_SVG: LazyLock<Cow<'static, [u8]>> = LazyLock::new(|| {
    Assets::get("favicon.svg")
        .map(|f| f.data)
        .unwrap_or_default()
});
/// The bundled page script (`web/src/` → `assets/app.js`; built by
/// `npm run build`, kept in sync by the `web` mise task). It replaces
/// the per-template inline `<script>` blocks; templates inline it via
/// `crate::html::app_js()`.
static APP_JS: LazyLock<String> = LazyLock::new(|| asset_string("app.js"));

/// `app.js` for template injection (`{{ crate::html::app_js()|safe }}`).
pub(crate) fn app_js() -> &'static str {
    APP_JS.as_str()
}

/// `GET /favicon.ico`: the embedded SVG site icon. Browsers request this
/// path on every page load; wave-0 verification saw it 404 each time (#87).
pub async fn favicon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG.clone(),
    )
        .into_response()
}

/// `GET /opensearch.xml` (W2-11): the OpenSearch 1.1 description document
/// browsers fetch after seeing the page head's `<link rel="search">`.
///
/// The absolute URL templates use the configured canonical public origin,
/// or the effective bind host and port when no public origin is configured.
/// Request `Host` and forwarded headers are never used as URL input.
pub async fn opensearch(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    (
        [(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml",
        )],
        opensearch_xml(&origin),
    )
        .into_response()
}

fn opensearch_xml(origin: &str) -> String {
    let results_url = xml_attribute_escape(&format!("{origin}/search?q={{searchTerms}}"));
    let suggestions_url = xml_attribute_escape(&format!("{origin}/api/suggest?q={{searchTerms}}"));
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\">\n",
            "  <ShortName>cauce</ShortName>\n",
            "  <Description>cauce metasearch</Description>\n",
            "  <InputEncoding>UTF-8</InputEncoding>\n",
            "  <Url type=\"text/html\" rel=\"results\" \
             template=\"{results_url}\"/>\n",
            "  <Url type=\"application/x-suggestions+json\" rel=\"suggestions\" \
             template=\"{suggestions_url}\"/>\n",
            "</OpenSearchDescription>\n",
        ),
        results_url = results_url,
        suggestions_url = suggestions_url
    )
}

fn xml_attribute_escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::xml_attribute_escape;

    #[test]
    fn xml_attribute_values_escape_markup_delimiters() {
        assert_eq!(
            xml_attribute_escape("https://search.localhost/?q=\"a&b'<x>"),
            "https://search.localhost/?q=&quot;a&amp;b&apos;&lt;x&gt;"
        );
    }
}
