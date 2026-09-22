//! The typed JSON error envelope every route shares:
//! `{"error": {"code", "message", "request_id"}}` (W0-09 spec).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

/// An API error rendered as the `{error: {...}}` envelope.
///
/// `request_id` is stamped by the handler's [`RequestCtx`] helpers
/// (`ctx.bad_request(...)` etc.) so the envelope id always equals the
/// `X-Request-Id` response header.
///
/// [`RequestCtx`]: crate::middleware::RequestCtx
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    /// Stable machine code (`bad_request`, `not_found`, `no_engines`, ...).
    code: &'static str,
    message: String,
    request_id: Option<Uuid>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            request_id: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// `404` variant of `method_not_allowed` fallbacks and friends.
    pub fn method_not_allowed(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            message,
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }

    /// A `Store` failure. The store's message carries SQL/IO detail (never
    /// secrets), so it is passed through for the operator.
    pub fn store(e: &oxe_core::StoreError) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            e.to_string(),
        )
    }

    /// Stamp the request id into the envelope.
    pub fn with_request_id(mut self, request_id: Option<Uuid>) -> Self {
        self.request_id = request_id;
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "request_id": self.request_id,
            }
        });
        (self.status, Json(body)).into_response()
    }
}
