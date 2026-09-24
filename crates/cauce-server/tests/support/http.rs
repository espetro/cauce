//! `oneshot` request helpers over a `Router`: `call` is the plain text
//! fetch, `call_json`/`get_json` parse the body, `get`/`get_html` are the
//! GET views, `put_form` is the settings form PUT, and `assert_envelope`
//! checks the `{"error": ...}` shape.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

/// An empty-body request for `method` on `uri`.
#[allow(dead_code)]
pub fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

/// status + body text.
#[allow(dead_code)]
pub async fn call(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// status + headers + JSON-parsed body (`Null` on non-JSON bodies).
#[allow(dead_code)]
pub async fn call_json(router: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, body)
}

/// Bare GET: status + headers + body text (no `Accept` override).
#[allow(dead_code)]
pub async fn get_headers(router: &Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let resp = router
        .clone()
        .oneshot(req("GET", uri))
        .await
        .expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// Bare GET: status + headers + JSON-parsed body (`Null` on non-JSON).
#[allow(dead_code)]
pub async fn get(router: &Router, uri: &str) -> (StatusCode, HeaderMap, Value) {
    call_json(router, req("GET", uri)).await
}

/// GET with an explicit `Accept` header: status + body text.
#[allow(dead_code)]
pub async fn get_with(router: &Router, uri: &str, accept: &str) -> (StatusCode, String) {
    call(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", accept)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// GET with `Accept: text/html`: status + body text.
#[allow(dead_code)]
pub async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    get_with(router, uri, "text/html").await
}

/// GET with `Accept: application/json`: status + parsed body.
#[allow(dead_code)]
pub async fn get_json(router: &Router, uri: &str) -> (StatusCode, Value) {
    let (status, _headers, body) = call_json(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "application/json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    (status, body)
}

/// `PUT /api/config` with an urlencoded form body, as htmx sends it.
#[allow(dead_code)]
pub async fn put_form(router: &Router, body: &str, hx: bool) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(Method::PUT)
        .uri("/api/config")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("X-Cauce-Client", "ui");
    if hx {
        req = req.header("HX-Request", "true");
    }
    call(router, req.body(Body::from(body.to_string())).unwrap()).await
}

/// `{"error": {code, message, request_id}}` assertion helper.
#[allow(dead_code)]
pub fn assert_envelope(body: &Value, code: &str) {
    let error = &body["error"];
    assert_eq!(error["code"], code, "envelope code: {body}");
    assert!(error["message"].is_string(), "envelope message: {body}");
    assert!(
        error["request_id"]
            .as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .is_some(),
        "envelope request_id must be a uuid: {body}"
    );
}
