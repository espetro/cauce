//! Per-request middleware: `RequestId` minting, `ClientKind` detection and
//! the `X-Request-Id` response header (parent plan 6.1).
//!
//! Every inbound request enters a [`request_span`] for its duration, so the
//! JSONL layer can hoist `request_id` onto every event in the request tree
//! and `oxe trace <id>` can rebuild it. An inbound `X-Request-Id` header is
//! honoured when it parses as a UUID; otherwise a fresh UUIDv7 is minted.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use oxe_core::ClientKind;
use tracing::Instrument;
use uuid::Uuid;

use crate::error::ApiError;
use crate::observability::{RequestId, request_span};

/// Per-request context the middleware installs into request extensions and
/// handlers extract with `Extension<RequestCtx>`.
#[derive(Debug, Clone)]
pub struct RequestCtx {
    pub request_id: RequestId,
    pub client: ClientKind,
}

impl RequestCtx {
    /// `err` with the request id already stamped on the envelope.
    pub fn err(
        &self,
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> ApiError {
        ApiError::new(status, code, message).with_request_id(Some(self.request_id.as_uuid()))
    }

    pub fn bad_request(&self, message: impl Into<String>) -> ApiError {
        self.err(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn not_found(&self, message: impl Into<String>) -> ApiError {
        self.err(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn store(&self, e: &oxe_core::StoreError) -> ApiError {
        ApiError::store(e).with_request_id(Some(self.request_id.as_uuid()))
    }

    /// Actor label for audit rows (plan 6.1): the `X-Actor` header when
    /// present and non-empty, else the client-kind label
    /// (`ui | api | mcp:<client> | cli`).
    pub fn actor(&self, headers: &HeaderMap) -> String {
        headers
            .get("x-actor")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.client.label())
    }
}

/// axum `from_fn` middleware: mint or honour `RequestId`, derive
/// `ClientKind`, install [`RequestCtx`], echo `X-Request-Id` on the
/// response.
pub async fn request_context(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<Uuid>().ok())
        .map(RequestId::from_uuid)
        .unwrap_or_default();
    let client = client_kind(request.headers(), request.uri().path());

    let span = request_span(request_id);
    span.record("client", tracing::field::display(client.label()));
    span.record("method", tracing::field::display(request.method().as_str()));
    span.record("path", tracing::field::display(request.uri().path()));

    request
        .extensions_mut()
        .insert(RequestCtx { request_id, client });

    let mut response = next.run(request).instrument(span).await;
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&request_id.to_string()).expect("a UUID is valid header text"),
    );
    response
}

/// `ClientKind` from path and headers (W0-09 spec): an explicit
/// `X-Oxe-Client` header wins (`ui`, `api`, `cli`, `mcp` / `mcp:<name>`),
/// else `/api/*`, `/health`, `/metrics` and `/mcp` get their surface kind
/// and everything else is `ui`. For `/mcp` the middleware only knows the
/// surface: the real client name arrives with the MCP `initialize`
/// handshake, so the span/audit kind here is `mcp:unknown` and the tool
/// layer re-derives `ClientKind::Mcp(name)` per call.
fn client_kind(headers: &HeaderMap, path: &str) -> ClientKind {
    if let Some(v) = headers
        .get("x-oxe-client")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let lower = v.to_ascii_lowercase();
        match lower.as_str() {
            "ui" | "web" => return ClientKind::Ui,
            "api" => return ClientKind::Api,
            "cli" => return ClientKind::Cli,
            "mcp" => return ClientKind::Mcp("unknown".to_string()),
            _ => {}
        }
        if let Some(name) = lower.strip_prefix("mcp:") {
            let name = name.trim();
            return ClientKind::Mcp(if name.is_empty() {
                "unknown".to_string()
            } else {
                name.to_string()
            });
        }
    }
    if path == "/mcp" {
        return ClientKind::Mcp("unknown".to_string());
    }
    if path.starts_with("/api/") || path == "/health" || path == "/metrics" {
        ClientKind::Api
    } else {
        ClientKind::Ui
    }
}
