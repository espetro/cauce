//! `cauce serve --headless` e2e (W1-12 acceptance).
//!
//! Runs the real `cauce` binary with `--headless` and asserts the
//! `requires: "ui"` routes are absent while the JSON surface stays up. The
//! test only exists in `ui` builds: without the feature the pages are
//! compiled out and `--headless` cannot be told apart from it.

//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

#![cfg(feature = "ui")]

use tempfile::TempDir;

use crate::common;

#[tokio::test]
async fn headless_drops_pages_keeps_api() {
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let server = common::spawn_cauce_bin_full(
        common::cauce_bin(),
        &data_dir,
        &config_dir,
        "replay",
        &[],
        &["--headless"],
    )
    .await;
    let addr = server.addr;

    // The HTMX pages are unmounted: they 404 with the error envelope.
    for uri in ["/", "/search?q=headless"] {
        let (status, body) = common::http(addr, "GET", uri, None).await;
        assert_eq!(status, 404, "{uri} must 404 under --headless: {body}");
        assert!(body.contains("not_found"), "{uri}: {body}");
    }

    // The JSON API and health stay up.
    let (status, body) = common::http(addr, "GET", "/api/search?q=headless", None).await;
    assert_eq!(status, 200, "api search under --headless failed: {body}");
    let (status, body) = common::http(addr, "GET", "/health", None).await;
    assert_eq!(status, 200, "health under --headless failed: {body}");

    // `/mcp` is not a `ui` surface: it stays mounted headless. A bare POST
    // without the MCP headers answers 4xx, which proves the route exists.
    if cfg!(feature = "mcp") {
        let (status, body) = common::http(addr, "POST", "/mcp", Some("{}")).await;
        assert_ne!(
            status, 404,
            "/mcp must stay mounted under --headless: {body}"
        );
    }

    server.shutdown().await.expect("shutdown");
}
