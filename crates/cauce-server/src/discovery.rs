//! Agent discovery and machine-readable metadata endpoints.
//!
//! Provides RFC 8288 link headers, robots.txt with Content-Signals,
//! sitemap.xml, RFC 9727 API catalog, MCP server card (SEP-1649),
//! RFC v0.2.0 agent skills index, ARD manifest (ai-catalog.json),
//! A2A agent card, and OAuth / Auth.md discovery documents.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};

use crate::app::{AppState, RouterOptions};

pub const AGENT_LINK_HEADER: &str = "</.well-known/api-catalog>; rel=\"api-catalog\", </.well-known/mcp/server-card.json>; rel=\"service-desc\", </.well-known/agent-skills/index.json>; rel=\"describedby\", </.well-known/ai-catalog.json>; rel=\"service-desc\", <https://docs.cauce.fyi>; rel=\"service-doc\"";

pub const MARKDOWN_HOME: &str = r#"# cauce — The Search Layer You Own

> Metasearch and grounded AI answers for humans and agents over a shared TTL cache in a single Rust binary.

## Key Features

- **One binary, three surfaces**: Web UI, HTTP JSON API, and Model Context Protocol (MCP) server.
- **Shared cache**: Human lookups and AI agent queries share the same SQLite TTL cache on disk.
- **Grounded answers**: AI-generated answers citing evidence from search results, or admitting when evidence is lacking.
- **Pluggable engines**: Declarative YAML search engines (DuckDuckGo, Brave, Google, Wikipedia, Hacker News, and more).
- **Fast and lightweight**: Tail-tolerant fan-out, circuit breakers, hedging, <80MB RSS idle.

## Quick Start

### Installation

```bash
curl -fsSL https://cauce.fyi/install.sh | bash
```

Or via Cargo:
```bash
cargo install --git https://github.com/espetro/cauce.git cauce-cli
```

### Running the Server

```bash
# Run web UI, API, and MCP
cauce serve

# Run headless (API + MCP only)
cauce serve --headless
```

## Surfaces & Interfaces

- **Web UI**: http://localhost:4479 or https://cauce.fyi
- **HTTP API**:
  - `GET /api/search?q={query}` — Search across configured engines
  - `GET /api/suggest?q={prefix}` — Frecency search suggestions
  - `POST /api/answer` — Grounded AI answer stream
  - `GET /health` — Instance health
- **Model Context Protocol (MCP)**:
  - Streamable HTTP endpoint: `/mcp`
  - Stdio transport: `cauce mcp`
- **Documentation**: https://docs.cauce.fyi
- **Machine-readable docs**: https://docs.cauce.fyi/llms.txt

## Agent Discovery

- **MCP Server Card**: `/.well-known/mcp/server-card.json`
- **API Catalog**: `/.well-known/api-catalog`
- **Agent Skills**: `/.well-known/agent-skills/index.json`
- **AI Catalog (ARD)**: `/.well-known/ai-catalog.json`
- **Agent Registration / Auth**: `/auth.md`
"#;

pub const SKILL_SEARCH_MD: &str = r#"# cauce Search Skill

Metasearch across multiple engines with deduplication, ranking, and grounded citations over a shared TTL cache.

## Endpoints

- MCP Streamable HTTP: `POST /mcp`
- HTTP JSON API: `GET /api/search?q={query}`

## Parameters

- `q`: Search query string (required)
- `page`: Page number (default: 1)
- `lang`: Language code (optional, e.g. "en")
- `time_range`: Time filter (optional: "day", "week", "month", "year")
- `safesearch`: Safe search level (0: off, 1: moderate, 2: strict)

## Response

Returns JSON with ranked results including title, URL, snippet, engine source, and cached flag.
"#;

pub const SKILL_SEARCH_SHA256: &str =
    "ad8e671ce74939448bdd075f3499ea84ab9dbc14351cf82257425ba47dfa46b3";

pub const AUTH_MD: &str = r#"# cauce auth.md

## Agent Registration and Authentication

cauce is a local and self-hosted metasearch engine and MCP server with a shared TTL cache.

### Public Access
- Search endpoints (`GET /api/search`, `GET /api/suggest`) and MCP discovery are publicly accessible without credentials on public instances.
- Rate limiting is enforced per IP/client token.

### Protected Operations & Admin Access
- Administrative endpoints (`/api/config`, `/api/cache`, `/api/engines`) and mutating MCP operations (`cache_invalidate`, `fetch_and_index`) require an operator Bearer token:
  ```http
  Authorization: Bearer <admin-token>
  ```
- To provision or configure tokens, refer to the server configuration in `cauce.toml`:
  ```toml
  [server]
  admin_tokens = ["your-secret-token"]
  ```

### API Discovery
- API Catalog: `/.well-known/api-catalog`
- MCP Server Card: `/.well-known/mcp/server-card.json`
- Documentation: https://docs.cauce.fyi
"#;

pub fn prefers_markdown(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|accept| accept.contains("text/markdown"))
        .unwrap_or(false)
}

pub fn markdown_response() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (header::LINK, AGENT_LINK_HEADER),
            (header::HeaderName::from_static("x-markdown-tokens"), "318"),
        ],
        MARKDOWN_HOME,
    )
        .into_response()
}

pub async fn robots_txt(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        "User-agent: *\nAllow: /\nContent-Signal: ai-train=yes, search=yes, ai-input=yes\n\nSitemap: {origin}/sitemap.xml\nAgentmap: {origin}/.well-known/ai-catalog.json\n"
    );
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn sitemap_xml(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
            "  <url>\n",
            "    <loc>{origin}/</loc>\n",
            "    <changefreq>daily</changefreq>\n",
            "    <priority>1.0</priority>\n",
            "  </url>\n",
            "  <url>\n",
            "    <loc>{origin}/app</loc>\n",
            "    <changefreq>daily</changefreq>\n",
            "    <priority>0.9</priority>\n",
            "  </url>\n",
            "  <url>\n",
            "    <loc>{origin}/app/search</loc>\n",
            "    <changefreq>daily</changefreq>\n",
            "    <priority>0.8</priority>\n",
            "  </url>\n",
            "</urlset>\n",
        ),
        origin = origin
    );
    (
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn auth_md() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        AUTH_MD,
    )
        .into_response()
}

pub async fn api_catalog(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "linkset": [
    {{
      "anchor": "{origin}/api/search",
      "service-desc": [
        {{
          "href": "https://docs.cauce.fyi/api",
          "type": "text/html"
        }}
      ],
      "service-doc": [
        {{
          "href": "https://docs.cauce.fyi",
          "type": "text/html"
        }}
      ],
      "status": [
        {{
          "href": "{origin}/health"
        }}
      ]
    }},
    {{
      "anchor": "{origin}/mcp",
      "service-desc": [
        {{
          "href": "{origin}/.well-known/mcp/server-card.json",
          "type": "application/json"
        }}
      ],
      "service-doc": [
        {{
          "href": "https://docs.cauce.fyi/agents",
          "type": "text/html"
        }}
      ]
    }}
  ]
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/linkset+json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn mcp_server_card(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "serverInfo": {{
    "name": "cauce",
    "version": "0.9.2"
  }},
  "description": "Local metasearch and grounded AI answers for humans and agents over a shared cache.",
  "url": "{origin}/mcp",
  "transport": {{
    "type": "streamable-http"
  }},
  "capabilities": {{
    "tools": true
  }}
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn agent_skills_index(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "$schema": "https://schemas.agentskills.io/discovery/0.2.0/schema.json",
  "skills": [
    {{
      "name": "search",
      "type": "skill-md",
      "description": "Search the web via cauce metasearch and retrieve ranked results with citations.",
      "url": "{origin}/.well-known/agent-skills/search/SKILL.md",
      "digest": "sha256:{SKILL_SEARCH_SHA256}"
    }}
  ]
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn agent_skills_search_md() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        SKILL_SEARCH_MD,
    )
        .into_response()
}

pub async fn a2a_agent_card(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "name": "cauce",
  "version": "0.9.2",
  "description": "Metasearch and grounded AI answer engine for humans and agents.",
  "url": "{origin}",
  "supportedInterfaces": [
    {{
      "url": "{origin}/mcp",
      "protocol": "mcp"
    }},
    {{
      "url": "{origin}/api/search",
      "protocol": "http"
    }}
  ],
  "capabilities": {{
    "search": true,
    "groundedAnswers": true
  }},
  "skills": [
    {{
      "id": "search",
      "name": "Web Search",
      "description": "Aggregate search across multiple search engines with source citations."
    }}
  ]
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn ai_catalog(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let host = origin
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let body = format!(
        r#"{{
  "specVersion": "1.0",
  "host": {{
    "displayName": "cauce",
    "identifier": "did:web:{host}"
  }},
  "entries": [
    {{
      "identifier": "urn:air:{host}:server:mcp",
      "displayName": "cauce MCP Search Server",
      "type": "application/mcp-server-card+json",
      "url": "{origin}/.well-known/mcp/server-card.json",
      "representativeQueries": [
        "search the web for rust async tutorials",
        "find latest tech news and documentation",
        "metasearch across duckduckgo and brave",
        "grounded web search with citations"
      ]
    }},
    {{
      "identifier": "urn:air:{host}:api:catalog",
      "displayName": "cauce API Catalog",
      "type": "application/linkset+json",
      "url": "{origin}/.well-known/api-catalog",
      "representativeQueries": [
        "cauce http api discovery",
        "search endpoints health and specs"
      ]
    }}
  ]
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn oauth_authorization_server(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "issuer": "{origin}",
  "authorization_endpoint": "{origin}/oauth/authorize",
  "token_endpoint": "{origin}/oauth/token",
  "jwks_uri": "{origin}/.well-known/jwks.json",
  "response_types_supported": ["token", "code"],
  "grant_types_supported": ["client_credentials", "authorization_code"],
  "scopes_supported": ["search", "admin"],
  "agent_auth": {{
    "skill": "https://isitagentready.com/.well-known/agent-skills/auth-md/SKILL.md",
    "register_uri": "{origin}/auth.md",
    "identity_types_supported": ["anonymous", "identity_assertion"],
    "anonymous": {{
      "credential_types_supported": ["bearer_token"],
      "claim_uri": "{origin}/auth.md"
    }}
  }}
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}

pub async fn oauth_protected_resource(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    let body = format!(
        r#"{{
  "resource": "{origin}",
  "authorization_servers": ["{origin}"],
  "scopes_supported": ["search", "admin"],
  "bearer_methods_supported": ["header"]
}}"#
    );
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}
