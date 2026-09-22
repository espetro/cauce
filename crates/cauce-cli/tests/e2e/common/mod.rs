//! Shared helpers for the cauce end-to-end tests in `tests/e2e/`.
//!
//! Uses a tiny raw HTTP/1.1 client over `tokio::net::TcpStream` so the tests
//! do not need `reqwest` or another large HTTP client crate.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::timeout;

/// Workspace root: `crates/cauce-cli` -> `crates` -> root.
pub fn workspace_root() -> &'static Path {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap().parent().unwrap()
}

/// The `cauce` binary built for the current test profile.
pub fn cauce_bin() -> &'static str {
    env!("CARGO_BIN_EXE_cauce")
}

/// A spawned `cauce serve` process and its bound address.
pub struct ServerGuard {
    pub addr: SocketAddr,
    child: Child,
}

impl ServerGuard {
    pub fn pid(&self) -> u32 {
        self.child.id().expect("child pid")
    }

    pub async fn shutdown(mut self) -> std::io::Result<()> {
        self.child.kill().await?;
        self.child.wait().await?;
        Ok(())
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        // Do not wait synchronously; the test calls `shutdown().await`.
        let _ = self.child.start_kill();
    }
}

pub async fn spawn_cauce(
    data_dir: impl AsRef<Path>,
    config_dir: impl AsRef<Path>,
    engines: &str,
) -> ServerGuard {
    spawn_cauce_bin(cauce_bin(), data_dir, config_dir, engines).await
}

/// [`spawn_cauce`] plus extra env vars (e.g. `CAUCE_REPLAY_BLOCKED` for the
/// breaker tests).
pub async fn spawn_cauce_env(
    data_dir: impl AsRef<Path>,
    config_dir: impl AsRef<Path>,
    engines: &str,
    extra_env: &[(&str, &str)],
) -> ServerGuard {
    spawn_cauce_bin_env(cauce_bin(), data_dir, config_dir, engines, extra_env).await
}

pub async fn spawn_cauce_bin(
    bin: &str,
    data_dir: impl AsRef<Path>,
    config_dir: impl AsRef<Path>,
    engines: &str,
) -> ServerGuard {
    spawn_cauce_bin_full(bin, data_dir, config_dir, engines, &[], &[]).await
}

pub async fn spawn_cauce_bin_env(
    bin: &str,
    data_dir: impl AsRef<Path>,
    config_dir: impl AsRef<Path>,
    engines: &str,
    extra_env: &[(&str, &str)],
) -> ServerGuard {
    spawn_cauce_bin_full(bin, data_dir, config_dir, engines, extra_env, &[]).await
}

/// Full form: extra env vars plus extra `cauce serve` args.
pub async fn spawn_cauce_bin_full(
    bin: &str,
    data_dir: impl AsRef<Path>,
    config_dir: impl AsRef<Path>,
    engines: &str,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
) -> ServerGuard {
    let mut cmd = Command::new(bin);
    cmd.current_dir(workspace_root())
        .arg("serve")
        .arg("--bind")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .args(extra_args)
        .env("CAUCE_DATA_DIR", data_dir.as_ref().as_os_str())
        .env("CAUCE_CONFIG_DIR", config_dir.as_ref().as_os_str())
        .env("CAUCE_ENGINES", engines)
        .env("CAUCE_LOG", "info")
        .env("CAUCE_LOG_PRETTY", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("failed to spawn cauce serve");

    let stderr = child.stderr.take().expect("stderr piped");
    let mut lines = BufReader::new(stderr).lines();
    let addr = timeout(Duration::from_secs(15), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(pos) = line.find("addr:") {
                let token = line[pos + 5..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(',');
                if let Ok(a) = token.parse::<SocketAddr>() {
                    return a;
                }
            }
        }
        panic!("server did not log its address");
    })
    .await
    .expect("timeout waiting for cauce serve");

    // Drain the rest of stderr so the pipe does not backpressure.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    ServerGuard { addr, child }
}

/// Raw HTTP/1.1 request over TCP. Returns `(status, body)`.
pub async fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect failed");

    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n",
        port = addr.port()
    );
    if let Some(b) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    request.push_str("\r\n");
    if let Some(b) = body {
        request.push_str(b);
    }

    stream.write_all(request.as_bytes()).await.expect("write");
    stream.flush().await.expect("flush");

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    timeout(Duration::from_secs(15), reader.read_line(&mut status_line))
        .await
        .expect("status timeout")
        .expect("status read");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("invalid HTTP status line: {status_line:?}"));

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        timeout(Duration::from_secs(15), reader.read_line(&mut line))
            .await
            .expect("header timeout")
            .expect("header read");
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse::<usize>().ok();
        }
    }

    let mut body_buf = Vec::new();
    if let Some(len) = content_length {
        body_buf.resize(len, 0);
        timeout(Duration::from_secs(15), reader.read_exact(&mut body_buf))
            .await
            .expect("body timeout")
            .expect("body read");
    } else {
        timeout(Duration::from_secs(15), reader.read_to_end(&mut body_buf))
            .await
            .expect("body timeout")
            .expect("body read");
    }

    (status, String::from_utf8_lossy(&body_buf).to_string())
}
