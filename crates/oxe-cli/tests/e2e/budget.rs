//! Nightly budget test (W0-12).
//!
//! Builds a release `oxe` binary, checks it is under 30 MB, then measures
//! RSS after 100 replay searches. Only runs when `OXE_NIGHTLY=1`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::process::Stdio;

use tempfile::TempDir;
use tokio::process::Command;

use crate::common;

const THIRTY_MB: u64 = 30 * 1024 * 1024;
const EIGHTY_MB_KB: u64 = 80 * 1024;

#[tokio::test]
async fn release_binary_size_and_rss() {
    if std::env::var("OXE_NIGHTLY").ok().as_deref() != Some("1") {
        return;
    }

    let ws = common::workspace_root();
    // The release dep-graph build lives outside `target/` by default: `du`/
    // `cargo clean` on the main target dir stay honest, and every checkout/
    // worktree on the host shares one copy instead of each building its own.
    // CI sets OXE_BUDGET_TARGET_DIR back inside target/ so rust-cache covers
    // it; a local override is useful too.
    let target_dir = std::env::var_os("OXE_BUDGET_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .unwrap_or_else(|| ws.join("target"));
            base.join("oxe/e2e-budget")
        });
    let bin = target_dir.join("release/oxe");

    let ws_owned: PathBuf = ws.to_path_buf();
    let target_dir_owned = target_dir.clone();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("cargo")
            .current_dir(&ws_owned)
            .env("CARGO_TARGET_DIR", &target_dir_owned)
            .args(["build", "--release", "-p", "oxe-cli"])
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
        common::spawn_oxe_bin(bin.to_str().unwrap(), &data_dir, &config_dir, "replay").await;
    let addr = server.addr;

    for i in 0..100 {
        let (status, body) = common::http(addr, "GET", "/api/search?q=golden+path", None).await;
        assert_eq!(status, 200, "search {i} failed: {body}");
    }

    let rss = rss_kb(server.pid()).await;
    assert!(rss < EIGHTY_MB_KB, "RSS is {rss} KB, >= {EIGHTY_MB_KB} KB");

    server.shutdown().await.expect("shutdown");
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
