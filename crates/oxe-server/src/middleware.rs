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
use axum::extract::{Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use oxe_core::ClientKind;
use oxe_core::config::{host_part, is_loopback_host};
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

/// The allow list for the Host/Origin guard (W1-13), frozen at router
/// build: every loopback name (`localhost`, `*.localhost` portless
/// aliases, `127.0.0.0/8`, `::1`) plus the configured bind host, so a
/// `PUT /api/config` swapping `server.host` cannot widen what the live
/// listener accepts.
#[derive(Debug, Clone)]
pub struct HostGuard {
    bind_host: String,
}

impl HostGuard {
    pub fn new(bind_host: &str) -> Self {
        Self {
            bind_host: host_part(bind_host).to_ascii_lowercase(),
        }
    }

    /// `authority` (a `Host` value or an `Origin`'s host) names this
    /// server when it is loopback or equals the configured bind host.
    fn allows(&self, authority: &str) -> bool {
        is_loopback_host(authority) || host_part(authority).eq_ignore_ascii_case(&self.bind_host)
    }
}

/// axum `from_fn_with_state` middleware (W1-13): the admin surface is
/// unauthenticated, so loopback is enforced at the request layer. A
/// foreign `Host` (the DNS-rebinding shape) is a 403 on any method; on
/// mutating methods a foreign `Origin` (the cross-site form/fetch shape)
/// is a 403. Absent headers pass — CLI and same-origin clients do not
/// always send them, and both attacks always carry them.
///
/// Runs inside [`request_context`], so rejections still carry
/// `X-Request-Id` and land in the request span.
pub async fn host_origin_guard(
    State(guard): State<HostGuard>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(host) = request.headers().get(HOST).and_then(|v| v.to_str().ok())
        && !guard.allows(host)
    {
        return forbidden(&request, format!("host {host:?} is not an oxe listen name"));
    }
    if is_mutating(request.method()) {
        let foreign = match request.headers().get(ORIGIN).and_then(|v| v.to_str().ok()) {
            None => false,
            Some(origin) => match origin.parse::<Uri>() {
                Ok(uri) => uri.host().is_none_or(|h| !guard.allows(h)),
                Err(_) => true, // unparseable Origin ("null" included) is foreign
            },
        };
        if foreign {
            return forbidden(&request, "origin is not same-host".to_string());
        }
    }
    next.run(request).await
}

/// Anything but `GET`/`HEAD`/`OPTIONS` can mutate; the strict reading
/// keeps exotic methods (TRACE, CONNECT, WebDAV verbs) behind the Origin
/// check too.
fn is_mutating(method: &axum::http::Method) -> bool {
    !matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    )
}

/// 403 envelope carrying the request id minted by `request_context`.
fn forbidden(request: &Request<Body>, reason: String) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestCtx>()
        .map(|c| c.request_id.as_uuid());
    ApiError::new(
        StatusCode::FORBIDDEN,
        "forbidden",
        format!("{reason}: this server only answers loopback clients"),
    )
    .with_request_id(request_id)
    .into_response()
}
