//! Tests for agent discovery, machine-readable metadata, and content negotiation.
//!
//! Covers: robots.txt, sitemap.xml, RFC 9727 API catalog, SEP-1649 MCP server card,
//! RFC v0.2.0 agent skills index, ARD manifest (ai-catalog.json), A2A agent card,
//! OAuth metadata, Auth.md, Link headers, and Markdown content negotiation.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

#![allow(unused_imports)]

use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, LINK};
use tower::ServiceExt;

mod support;
use support::*;

#[tokio::test]
async fn robots_txt_and_sitemap() {
    let (router, _state, _tmp) = app();

    // /robots.txt
    let (status, headers, body) = get_headers(&router, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "text/plain; charset=utf-8");
    assert!(body.contains("User-agent: *"));
    assert!(body.contains("Allow: /"));
    assert!(body.contains("Content-Signal: ai-train=yes, search=yes, ai-input=yes"));
    assert!(body.contains("Sitemap:"));
    assert!(body.contains("/sitemap.xml"));
    assert!(body.contains("Agentmap:"));
    assert!(body.contains("/.well-known/ai-catalog.json"));

    // /sitemap.xml
    let (status, headers, body) = get_headers(&router, "/sitemap.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/xml; charset=utf-8");
    assert!(body.contains("<urlset"));
    assert!(body.contains("<loc>"));
    assert!(body.contains("/app</loc>"));
}

#[tokio::test]
async fn auth_md_served() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get_headers(&router, "/auth.md").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "text/markdown; charset=utf-8");
    assert!(body.contains("# cauce auth.md"));
    assert!(body.contains("Agent Registration and Authentication"));
}

#[tokio::test]
async fn api_catalog_rfc9727() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/.well-known/api-catalog").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/linkset+json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert!(body["linkset"].is_array());
    assert!(!body["linkset"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn mcp_server_card() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/.well-known/mcp/server-card.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(body["serverInfo"]["name"], "cauce");
    assert_eq!(body["transport"]["type"], "streamable-http");
    assert_eq!(body["capabilities"]["tools"], true);
}

#[tokio::test]
async fn agent_skills_index_and_md() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/.well-known/agent-skills/index.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(
        body["$schema"],
        "https://schemas.agentskills.io/discovery/0.2.0/schema.json"
    );
    assert_eq!(body["skills"][0]["name"], "search");
    assert_eq!(body["skills"][0]["type"], "skill-md");

    // SKILL.md
    let (status, headers, body) =
        get_headers(&router, "/.well-known/agent-skills/search/SKILL.md").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "text/markdown; charset=utf-8");
    assert!(body.contains("# cauce Search Skill"));
}

#[tokio::test]
async fn a2a_agent_card() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/.well-known/agent-card.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(body["name"], "cauce");
    assert!(body["supportedInterfaces"].is_array());
}

#[tokio::test]
async fn ard_ai_catalog() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/.well-known/ai-catalog.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(body["specVersion"], "1.0");
    assert!(body["entries"].is_array());
    assert!(!body["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn oauth_and_prm_metadata() {
    let (router, _state, _tmp) = app();

    // oauth-authorization-server
    let (status, headers, body) = get(&router, "/.well-known/oauth-authorization-server").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert!(body["issuer"].is_string());
    assert!(body["agent_auth"].is_object());

    // oauth-protected-resource
    let (status, headers, body) = get(&router, "/.well-known/oauth-protected-resource").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(headers[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert!(body["resource"].is_string());
    assert!(body["authorization_servers"].is_array());
}

#[cfg(feature = "ui")]
#[tokio::test]
async fn markdown_content_negotiation_and_link_headers() {
    let (router, _state, _tmp) = app();

    // Standard request without Accept: text/markdown on /app returns HTML + Link header
    let (status, headers, _) = get_headers(&router, "/app").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key(LINK));
    let link = headers[LINK].to_str().unwrap();
    assert!(link.contains("rel=\"api-catalog\""));
    assert!(link.contains("rel=\"service-desc\""));

    // Request with Accept: text/markdown on /app returns Markdown
    let resp = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/app")
                .header("Accept", "text/markdown")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[CONTENT_TYPE], "text/markdown; charset=utf-8");
    assert_eq!(resp.headers()["x-markdown-tokens"], "318");
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body_str.contains("# cauce — The Search Layer You Own"));

    // Request with Accept: text/markdown on / returns Markdown directly
    let resp = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/")
                .header("Accept", "text/markdown")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[CONTENT_TYPE], "text/markdown; charset=utf-8");
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body_str.contains("# cauce — The Search Layer You Own"));
}
