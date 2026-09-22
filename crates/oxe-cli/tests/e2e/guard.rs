//! W1-13 e2e: `oxe serve` refuses a non-loopback bind while admin auth is
//! deferred, and the Host/Origin guard rejects foreign names on the wire.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::timeout;

use crate::common::{oxe_bin, spawn_oxe, workspace_root};

/// `oxe serve --bind 0.0.0.0` exits non-zero with a clear refusal: auth is
/// forced on a non-loopback bind but is not implemented yet.
#[tokio::test]
async fn non_loopback_bind_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let output = timeout(
        Duration::from_secs(15),
        Command::new(oxe_bin())
            .current_dir(workspace_root())
            .arg("serve")
            .arg("--bind")
            .arg("0.0.0.0")
            .arg("--port")
            .arg("0")
            .env("OXE_DATA_DIR", tmp.path().join("data"))
            .env("OXE_CONFIG_DIR", tmp.path().join("cfg"))
            .env("OXE_ENGINES", "replay")
            .output(),
    )
    .await
    .expect("refusal must not hang")
    .expect("spawn oxe serve");

    assert!(!output.status.success(), "expected refusal, got {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to bind 0.0.0.0"),
        "stderr: {stderr}"
    );
}

/// On the wire: a foreign `Host` is 403, a foreign `Origin` on a mutating
/// method is 403, and a plain loopback request still works.
#[tokio::test]
async fn foreign_host_and_origin_get_403() {
    let tmp = tempfile::tempdir().unwrap();
    let server = spawn_oxe(tmp.path().join("data"), tmp.path().join("cfg"), "replay").await;

    // Foreign Host on a read route.
    let (status, _) = raw_http(server.addr, "GET", "/health", "evil.example.com", None).await;
    assert_eq!(status, 403);

    // Foreign Origin on a mutating route (Host is fine).
    let host = format!("127.0.0.1:{}", server.addr.port());
    let (status, body) = raw_http(
        server.addr,
        "POST",
        "/api/click",
        &host,
        Some(("Origin: https://evil.example.com\r\n", "{}")),
    )
    .await;
    assert_eq!(status, 403);
    assert!(body.contains("\"forbidden\""), "{body}");

    // Same-host Origin on the mutating route reaches the handler.
    let origin = format!("Origin: http://{host}\r\n");
    let (status, _) = raw_http(
        server.addr,
        "POST",
        "/api/click",
        &host,
        Some((origin.as_str(), r#"{"url":"https://example.com"}"#)),
    )
    .await;
    assert_eq!(status, 204);

    // Plain loopback GET still answers.
    let (status, _) = raw_http(server.addr, "GET", "/health", &host, None).await;
    assert_eq!(status, 200);

    server.shutdown().await.unwrap();
}

/// Minimal raw request with explicit `Host` and an optional extra header
/// line + body; returns `(status, body)`.
async fn raw_http(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    host: &str,
    extra: Option<(&str, &str)>,
) -> (u16, String) {
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect failed");

    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some((headers, body)) = extra {
        request.push_str(headers);
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
        request.push_str("\r\n");
        request.push_str(body);
    } else {
        request.push_str("\r\n");
    }
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    timeout(Duration::from_secs(10), reader.read_line(&mut status_line))
        .await
        .expect("status timeout")
        .expect("status read");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status parse");

    // Drain headers, then read the body to EOF (Connection: close).
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}
