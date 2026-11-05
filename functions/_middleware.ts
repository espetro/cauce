// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

import type { ProxyEnv } from "./_proxy";

const AGENT_LINK_HEADER =
  '</.well-known/api-catalog>; rel="api-catalog", </.well-known/mcp/server-card.json>; rel="service-desc", </.well-known/agent-skills/index.json>; rel="describedby", </.well-known/ai-catalog.json>; rel="service-desc", <https://docs.cauce.fyi>; rel="service-doc"';

const MARKDOWN_HOME = `# cauce — The Search Layer You Own

> Metasearch and grounded AI answers for humans and agents over a shared TTL cache in a single Rust binary.

## Key Features

- **One binary, three surfaces**: Web UI, HTTP JSON API, and Model Context Protocol (MCP) server.
- **Shared cache**: Human lookups and AI agent queries share the same SQLite TTL cache on disk.
- **Grounded answers**: AI-generated answers citing evidence from search results, or admitting when evidence is lacking.
- **Pluggable engines**: Declarative YAML search engines (DuckDuckGo, Brave, Google, Wikipedia, Hacker News, and more).
- **Fast and lightweight**: Tail-tolerant fan-out, circuit breakers, hedging, <80MB RSS idle.

## Quick Start

### Installation

\`\`\`bash
curl -fsSL https://cauce.fyi/install.sh | bash
\`\`\`

Or via Cargo:
\`\`\`bash
cargo install --git https://github.com/espetro/cauce.git cauce-cli
\`\`\`

### Running the Server

\`\`\`bash
# Run web UI, API, and MCP
cauce serve

# Run headless (API + MCP only)
cauce serve --headless
\`\`\`

## Surfaces & Interfaces

- **Web UI**: http://localhost:4479 or https://cauce.fyi
- **HTTP API**:
  - \`GET /api/search?q={query}\` — Search across configured engines
  - \`GET /api/suggest?q={prefix}\` — Frecency search suggestions
  - \`POST /api/answer\` — Grounded AI answer stream
  - \`GET /health\` — Instance health
- **Model Context Protocol (MCP)**:
  - Streamable HTTP endpoint: \`/mcp\`
  - Stdio transport: \`cauce mcp\`
- **Documentation**: https://docs.cauce.fyi
- **Machine-readable docs**: https://docs.cauce.fyi/llms.txt

## Agent Discovery

- **MCP Server Card**: \`/.well-known/mcp/server-card.json\`
- **API Catalog**: \`/.well-known/api-catalog\`
- **Agent Skills**: \`/.well-known/agent-skills/index.json\`
- **AI Catalog (ARD)**: \`/.well-known/ai-catalog.json\`
- **Agent Registration / Auth**: \`/auth.md\`
`;

export const onRequest: PagesFunction<ProxyEnv> = async (context) => {
  const url = new URL(context.request.url);
  const accept = context.request.headers.get("accept") || "";

  // Content negotiation for Markdown for Agents
  if (
    (url.pathname === "/" || url.pathname === "/app" || url.pathname === "/index.html") &&
    accept.includes("text/markdown")
  ) {
    return new Response(MARKDOWN_HOME, {
      status: 200,
      headers: {
        "content-type": "text/markdown; charset=utf-8",
        "link": AGENT_LINK_HEADER,
        "x-markdown-tokens": "318",
        "cache-control": "public, max-age=3600",
      },
    });
  }

  const response = await context.next();

  // Ensure Link header on HTML pages
  const contentType = response.headers.get("content-type") || "";
  if (contentType.includes("text/html")) {
    const newHeaders = new Headers(response.headers);
    if (!newHeaders.has("link")) {
      newHeaders.set("link", AGENT_LINK_HEADER);
    }
    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: newHeaders,
    });
  }

  return response;
};
