#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

# CF Pages build for the cauce SPA (docs/deploy/cf-pages.md). The web UI
# is fully same-origin — every runtime call is a relative /api/* fetch —
# so the Pages project is only the built assets plus the /api proxy
# functions in the repo-root `functions/` dir (Pages reads functions from
# the checkout, not the output dir).
#
# Pages project settings:
#   Build command: bash deploy/cf-pages/build.sh
#   Output dir:    pages-dist
#   Env var:       API_ORIGIN=https://<backend origin, e.g. api.cauce.fyi>
#
# VITE_CAUCE_LANDING=1 bakes the landing bundle: the apex serves the
# app's home layout with the product sections below it (duckduckgo.com
# model). The binary's rust-embed build stays app-only — self-hosts get
# the plain layout.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

: "${API_ORIGIN:?set API_ORIGIN on the Pages project (e.g. https://api.cauce.fyi)}"

# The Pages image ships Node via nvm but no pnpm; corepack resolves the
# version pinned in package.json#packageManager.
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
corepack enable 2>/dev/null || npm install -g corepack
corepack prepare \
  "$(node -p "require('./crates/cauce-server/package.json').packageManager")" \
  --activate

pnpm --dir crates/cauce-server install --frozen-lockfile
VITE_CAUCE_LANDING=1 pnpm --dir crates/cauce-server build:spa

rm -rf pages-dist
mkdir -p pages-dist/app
cp -r crates/cauce-server/assets/spa/. pages-dist/app/

# Docs are NOT in this bundle: docs.cauce.fyi is a separate Pages
# project built by deploy/cf-pages/build-docs.sh off this same repo.

# `_redirects` carries static rules only — the API proxy lives in
# `functions/` because a 200-rewrite cannot target an external origin.
# No `/` redirect: the apex IS the landing (the SPA boots its home
# route there), duckduckgo.com-style.
cat > pages-dist/_redirects <<'EOF'
EOF

# No SPA-fallback rule is needed: without a top-level 404.html, Pages
# serves the root index.html for any unmatched path, which boots the
# app for deep links like /app/search?q=...
cp crates/cauce-server/assets/spa/index.html pages-dist/index.html

# Root statics the SPA shell links to (favicon, apple-touch, og image).
# `functions/favicon.ico.ts` is gone — a static file serves the same
# bytes with no origin dependency, which also works while the backend
# is absent. To swap the og image, copy another option from
# `assets/brand/og/` over `assets/brand/og.png`.
cp assets/brand/favicon.ico pages-dist/favicon.ico
cp assets/brand/apple-touch-icon.png pages-dist/apple-touch-icon.png
cp assets/brand/og.png pages-dist/og.png

# Agent discovery and machine-readable metadata
mkdir -p pages-dist/.well-known/mcp pages-dist/.well-known/agent-skills/search

cat > pages-dist/robots.txt <<'EOF'
User-agent: *
Allow: /
Content-Signal: ai-train=yes, search=yes, ai-input=yes

Sitemap: https://cauce.fyi/sitemap.xml
Agentmap: https://cauce.fyi/.well-known/ai-catalog.json
EOF

cat > pages-dist/sitemap.xml <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://cauce.fyi/</loc>
    <changefreq>daily</changefreq>
    <priority>1.0</priority>
  </url>
  <url>
    <loc>https://cauce.fyi/app</loc>
    <changefreq>daily</changefreq>
    <priority>0.9</priority>
  </url>
  <url>
    <loc>https://cauce.fyi/app/search</loc>
    <changefreq>daily</changefreq>
    <priority>0.8</priority>
  </url>
</urlset>
EOF

cat > pages-dist/auth.md <<'EOF'
# cauce auth.md

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
EOF

cat > pages-dist/.well-known/api-catalog <<'EOF'
{
  "linkset": [
    {
      "anchor": "https://cauce.fyi/api/search",
      "service-desc": [
        {
          "href": "https://docs.cauce.fyi/api",
          "type": "text/html"
        }
      ],
      "service-doc": [
        {
          "href": "https://docs.cauce.fyi",
          "type": "text/html"
        }
      ],
      "status": [
        {
          "href": "https://cauce.fyi/health"
        }
      ]
    },
    {
      "anchor": "https://cauce.fyi/mcp",
      "service-desc": [
        {
          "href": "https://cauce.fyi/.well-known/mcp/server-card.json",
          "type": "application/json"
        }
      ],
      "service-doc": [
        {
          "href": "https://docs.cauce.fyi/agents",
          "type": "text/html"
        }
      ]
    }
  ]
}
EOF

cat > pages-dist/.well-known/mcp/server-card.json <<'EOF'
{
  "serverInfo": {
    "name": "cauce",
    "version": "0.9.2"
  },
  "description": "Local metasearch and grounded AI answers for humans and agents over a shared cache.",
  "url": "https://cauce.fyi/mcp",
  "transport": {
    "type": "streamable-http"
  },
  "capabilities": {
    "tools": true
  }
}
EOF

cat > pages-dist/.well-known/agent-skills/index.json <<'EOF'
{
  "$schema": "https://schemas.agentskills.io/discovery/0.2.0/schema.json",
  "skills": [
    {
      "name": "search",
      "type": "skill-md",
      "description": "Search the web via cauce metasearch and retrieve ranked results with citations.",
      "url": "https://cauce.fyi/.well-known/agent-skills/search/SKILL.md",
      "digest": "sha256:ad8e671ce74939448bdd075f3499ea84ab9dbc14351cf82257425ba47dfa46b3"
    }
  ]
}
EOF

cat > pages-dist/.well-known/agent-skills/search/SKILL.md <<'EOF'
# cauce Search Skill

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
EOF

cat > pages-dist/.well-known/agent-card.json <<'EOF'
{
  "name": "cauce",
  "version": "0.9.2",
  "description": "Metasearch and grounded AI answer engine for humans and agents.",
  "url": "https://cauce.fyi",
  "supportedInterfaces": [
    {
      "url": "https://cauce.fyi/mcp",
      "protocol": "mcp"
    },
    {
      "url": "https://cauce.fyi/api/search",
      "protocol": "http"
    }
  ],
  "capabilities": {
    "search": true,
    "groundedAnswers": true
  },
  "skills": [
    {
      "id": "search",
      "name": "Web Search",
      "description": "Aggregate search across multiple search engines with source citations."
    }
  ]
}
EOF

cat > pages-dist/.well-known/ai-catalog.json <<'EOF'
{
  "specVersion": "1.0",
  "host": {
    "displayName": "cauce",
    "identifier": "did:web:cauce.fyi"
  },
  "entries": [
    {
      "identifier": "urn:air:cauce.fyi:server:mcp",
      "displayName": "cauce MCP Search Server",
      "type": "application/mcp-server-card+json",
      "url": "https://cauce.fyi/.well-known/mcp/server-card.json",
      "representativeQueries": [
        "search the web for rust async tutorials",
        "find latest tech news and documentation",
        "metasearch across duckduckgo and brave",
        "grounded web search with citations"
      ]
    },
    {
      "identifier": "urn:air:cauce.fyi:api:catalog",
      "displayName": "cauce API Catalog",
      "type": "application/linkset+json",
      "url": "https://cauce.fyi/.well-known/api-catalog",
      "representativeQueries": [
        "cauce http api discovery",
        "search endpoints health and specs"
      ]
    }
  ]
}
EOF

cat > pages-dist/.well-known/oauth-authorization-server <<'EOF'
{
  "issuer": "https://cauce.fyi",
  "authorization_endpoint": "https://cauce.fyi/oauth/authorize",
  "token_endpoint": "https://cauce.fyi/oauth/token",
  "jwks_uri": "https://cauce.fyi/.well-known/jwks.json",
  "response_types_supported": ["token", "code"],
  "grant_types_supported": ["client_credentials", "authorization_code"],
  "scopes_supported": ["search", "admin"],
  "agent_auth": {
    "skill": "https://isitagentready.com/.well-known/agent-skills/auth-md/SKILL.md",
    "register_uri": "https://cauce.fyi/auth.md",
    "identity_types_supported": ["anonymous", "identity_assertion"],
    "anonymous": {
      "credential_types_supported": ["bearer_token"],
      "claim_uri": "https://cauce.fyi/auth.md"
    }
  }
}
EOF

cat > pages-dist/.well-known/oauth-protected-resource <<'EOF'
{
  "resource": "https://cauce.fyi",
  "authorization_servers": ["https://cauce.fyi"],
  "scopes_supported": ["search", "admin"],
  "bearer_methods_supported": ["header"]
}
EOF

cat > pages-dist/_headers <<'EOF'
/*
  Link: </.well-known/api-catalog>; rel="api-catalog", </.well-known/mcp/server-card.json>; rel="service-desc", </.well-known/agent-skills/index.json>; rel="describedby", </.well-known/ai-catalog.json>; rel="service-desc", <https://docs.cauce.fyi>; rel="service-doc"

/.well-known/*
  Access-Control-Allow-Origin: *
  Cache-Control: public, max-age=3600

/.well-known/api-catalog
  Content-Type: application/linkset+json

/.well-known/mcp/server-card.json
  Content-Type: application/json

/.well-known/agent-skills/index.json
  Content-Type: application/json

/.well-known/agent-card.json
  Content-Type: application/json

/.well-known/ai-catalog.json
  Content-Type: application/json

/.well-known/oauth-authorization-server
  Content-Type: application/json

/.well-known/oauth-protected-resource
  Content-Type: application/json

/auth.md
  Content-Type: text/markdown; charset=utf-8

/robots.txt
  Content-Type: text/plain; charset=utf-8

/sitemap.xml
  Content-Type: application/xml; charset=utf-8
EOF

echo "pages-dist ready (API_ORIGIN=$API_ORIGIN)"
