//! Nightly budget test (W0-12, extended W1-12).
//!
//! Builds a release `cauce` binary, checks it is under 30 MB, then measures
//! RSS for each mode: `cauce serve` after 100 replay searches (< 80 MB),
//! `cauce serve --headless` (< 50 MB) and `cauce mcp` stdio (< 40 MB).
//! Only runs when `CAUCE_NIGHTLY=1`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tempfile::TempDir;
use tokio::process::Command;

use crate::common;

const THIRTY_MB: u64 = 30 * 1024 * 1024;
const EIGHTY_MB_KB: u64 = 80 * 1024;
const FIFTY_MB_KB: u64 = 50 * 1024;
const FORTY_MB_KB: u64 = 40 * 1024;

/// Build the release binary (shared `CAUCE_BUDGET_TARGET_DIR`, so repeat
/// invocations across tests/checkouts are cargo no-ops) and return its path.
async fn release_bin() -> PathBuf {
    let ws = common::workspace_root();
    // The release dep-graph build lives outside `target/` by default: `du`/
    // `cargo clean` on the main target dir stay honest, and every checkout/
    // worktree on the host shares one copy instead of each building its own.
    // CI sets CAUCE_BUDGET_TARGET_DIR back inside target/ so rust-cache covers
    // it; a local override is useful too.
    let target_dir = std::env::var_os("CAUCE_BUDGET_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .unwrap_or_else(|| ws.join("target"));
            base.join("cauce/e2e-budget")
        });
    let bin = target_dir.join("release/cauce");

    let ws_owned: PathBuf = ws.to_path_buf();
    let target_dir_owned = target_dir.clone();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("cargo")
            .current_dir(&ws_owned)
            .env("CARGO_TARGET_DIR", &target_dir_owned)
            .args(["build", "--release", "-p", "cauce-cli"])
            .output()
            .expect("cargo build");
        assert!(
            output.status.success(),
            "release build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    })
    .await
    .expect("spawn_blocking");

    bin
}

#[tokio::test]
async fn release_binary_size_and_rss() {
    if std::env::var("CAUCE_NIGHTLY").ok().as_deref() != Some("1") {
        return;
    }

    let bin = release_bin().await;
    let size = std::fs::metadata(&bin)
        .unwrap_or_else(|_| panic!("release binary not found at {}", bin.display()))
        .len();
    assert!(
        size < THIRTY_MB,
        "release binary is {size} bytes, >= {THIRTY_MB} bytes"
    );

    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let server =
        common::spawn_cauce_bin(bin.to_str().unwrap(), &data_dir, &config_dir, "replay").await;
    let addr = server.addr;

    for i in 0..100 {
        let (status, body) = common::http(addr, "GET", "/api/search?q=golden+path", None).await;
        assert_eq!(status, 200, "search {i} failed: {body}");
    }

    let rss = rss_kb(server.pid()).await;
    assert!(rss < EIGHTY_MB_KB, "RSS is {rss} KB, >= {EIGHTY_MB_KB} KB");

    server.shutdown().await.expect("shutdown");
}

/// W1-12 mode budgets: `cauce serve --headless` idles under 50 MB RSS and
/// `cauce mcp` (stdio, no listener) under 40 MB.
#[tokio::test]
async fn release_headless_and_mcp_rss() {
    if std::env::var("CAUCE_NIGHTLY").ok().as_deref() != Some("1") {
        return;
    }
    let bin = release_bin().await;
    let tmp = TempDir::new().expect("tempdir");

    // `cauce serve --headless`: API surface only, warmed by a few searches.
    let server = common::spawn_cauce_bin_full(
        bin.to_str().unwrap(),
        tmp.path().join("headless-data"),
        tmp.path().join("headless-cfg"),
        "replay",
        &[],
        &["--headless"],
    )
    .await;
    for i in 0..10 {
        let (status, body) = common::http(server.addr, "GET", "/api/search?q=budget", None).await;
        assert_eq!(status, 200, "headless search {i} failed: {body}");
    }
    let rss = rss_kb(server.pid()).await;
    assert!(
        rss < FIFTY_MB_KB,
        "headless RSS is {rss} KB, >= {FIFTY_MB_KB} KB"
    );
    server.shutdown().await.expect("shutdown");

    // `cauce mcp`: stdio transport, no listener. stdin stays piped open so
    // the transport idles instead of seeing EOF and exiting.
    let mut child = Command::new(&bin)
        .arg("mcp")
        .current_dir(common::workspace_root())
        .env("CAUCE_DATA_DIR", tmp.path().join("mcp-data"))
        .env("CAUCE_CONFIG_DIR", tmp.path().join("mcp-cfg"))
        .env("CAUCE_ENGINES", "replay")
        .env("CAUCE_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cauce mcp");
    // Let init (store open, engine wiring) finish and RSS settle.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let pid = child.id().expect("cauce mcp pid");
    let rss = rss_kb(pid).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    assert!(
        rss < FORTY_MB_KB,
        "cauce mcp RSS is {rss} KB, >= {FORTY_MB_KB} KB"
    );
}

async fn rss_kb(pid: u32) -> u64 {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .stdout(Stdio::piped())
        .output()
        .await
        .expect("ps");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("cannot parse RSS for pid {pid}"))
}
