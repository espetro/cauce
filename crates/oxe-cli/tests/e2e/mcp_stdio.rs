//! `oxe mcp` stdio e2e (W1-08 acceptance 3).
//!
//! Spawns the real `oxe` binary with `mcp` and tempdir data/config dirs +
//! `OXE_ENGINES=replay`, then drives the MCP protocol over the child's
//! stdin/stdout with an rmcp client: initialize, `tools/list` (exactly four
//! tools), `search_web` (replay results + `request_id`). Also asserts the
//! mode is stdio-only: the child logs "serving stdio" and never binds a
//! listener, and exits cleanly when the client closes the transport.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeSet;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, Implementation, InitializeRequestParams,
};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::common;

/// The settled W1-08 tool surface: exactly these four names.
const TOOL_NAMES: [&str; 4] = [
    "search_web",
    "cache_status",
    "cache_invalidate",
    "exa_search",
];

#[tokio::test]
async fn mcp_stdio_search() {
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let config_dir = tmp.path().join("cfg");

    let mut child = Command::new(common::oxe_bin())
        .current_dir(common::workspace_root())
        .arg("mcp")
        .env("OXE_DATA_DIR", data_dir.as_os_str())
        .env("OXE_CONFIG_DIR", config_dir.as_os_str())
        .env("OXE_ENGINES", "replay")
        .env("OXE_LOG", "info")
        .env("OXE_LOG_PRETTY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn oxe mcp");

    // The protocol channel: stdin/stdout of the child.
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    // Drain stderr into a buffer so the pipe never backpressures and the
    // test can assert on what the process logged (stdio-only, no listener).
    let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let lines = Arc::clone(&stderr_lines);
        let stderr = child.stderr.take().expect("stderr piped");
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.lock().unwrap().push(line);
            }
        });
    }

    // rmcp client over the (reader, writer) pair.
    let client: RunningService<RoleClient, InitializeRequestParams> =
        timeout(Duration::from_secs(30), async {
            InitializeRequestParams::new(
                ClientCapabilities::default(),
                Implementation::new("oxe-mcp-stdio-test", "0.0.0"),
            )
            .serve((stdout, stdin))
            .await
        })
        .await
        .expect("mcp initialize timed out")
        .expect("mcp initialize");

    // Exactly the four settled tools.
    let tools = timeout(Duration::from_secs(10), client.list_all_tools())
        .await
        .expect("tools/list timed out")
        .expect("tools/list");
    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let expected: BTreeSet<String> = TOOL_NAMES.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected, "tool surface must be exactly {expected:?}");

    // search_web against the replay engine, same assertions as the HTTP test.
    let result = timeout(
        Duration::from_secs(15),
        client.call_tool(
            CallToolRequestParams::new("search_web").with_arguments(
                json!({"query": "stdio probe", "engines": ["replay"]})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .expect("search_web timed out")
    .expect("call search_web");
    assert_ne!(result.is_error, Some(true), "tool error: {result:?}");
    let body: &Value = result
        .structured_content
        .as_ref()
        .expect("structured_content");
    body["meta"]["request_id"]
        .as_str()
        .expect("meta.request_id")
        .parse::<uuid::Uuid>()
        .expect("meta.request_id is a UUID");
    assert!(
        !body["results"].as_array().expect("results").is_empty(),
        "replay must return results: {body}"
    );

    // Closing the transport must end the child cleanly: EOF on stdin ends
    // the rmcp stdio loop and the process exits 0.
    client.cancel().await.expect("cancel");
    let status = timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("child did not exit after stdin close")
        .expect("child wait");
    assert!(status.success(), "oxe mcp exit status: {status}");

    // Stdio-only: the log shows the stdio banner and never a bound listener.
    let lines = stderr_lines.lock().unwrap();
    let joined = lines.join("\n");
    assert!(
        joined.contains("serving stdio"),
        "missing stdio banner; stderr:\n{joined}"
    );
    assert!(
        !joined.contains("listening") && !joined.contains("addr:"),
        "oxe mcp must not bind a listener; stderr:\n{joined}"
    );
}
