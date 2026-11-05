<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/.

     PMF sections rendered below the home omnibox when the bundle was
     built with VITE_CAUCE_LANDING=1 (see lib/flags.ts): the cauce.fyi
     deploy. English-only marketing copy by design; not routed through
     the i18n bundles. -->
<script lang="ts">
  import Code from "./Code.svelte";

  const apiSnippet = `# the same search the UI runs, as JSON
curl '${location.origin}/api/search?q=reciprocal+rank+fusion'
# POST /api/answer streams the grounded answer over SSE`;

  const apiResponse = `{
  "results": [ { "url": "…", "title": "…", "engine": "brave" } ],
  "meta": { "source": "network", "elapsed_ms": 348,
            "request_id": "01a11d4e-…" }
}`;

  const mcpConfig = `// ~/.claude/settings.json: four tools over streamable HTTP
{
  "mcpServers": {
    "search": { "type": "http", "url": "${location.origin}/mcp" }
  }
}
// tools: search_web, exa_search, cache_status, cache_invalidate`;

  const engineYaml = `# engines/brave.yaml (trimmed): a search engine as config
id: brave
tier: 1
page_size: 20
request:
  url: "https://search.brave.com/search?q={q}&offset={page0}&source=web"
  timeout_ms: 2500
parse:
  kind: html
  results: 'div.snippet[data-type="web"]'`;

  const installSnippet = `# 1. install script (macOS and Linux, no toolchain needed)
curl -fsSL https://raw.githubusercontent.com/espetro/cauce/main/install.sh | sh

# 2. mise
mise use -g github:espetro/cauce

# 3. from source, with cargo
cargo install --locked --git https://github.com/espetro/cauce cauce-cli

# then
cauce serve   # UI + API + MCP on http://127.0.0.1:4479`;

  const compareRows: [string, string, string][] = [
    ["where it runs", "your machine, loopback by default", "their cloud, your key"],
    ["pricing", "free, MPL-2.0; your upstream engines are the only limit", "per-call metering, quotas, keys"],
    ["data", "queries and answers stay on your disk", "queries transit their infra"],
    ["surfaces", "UI + API + MCP in one process", "API-first; you build the rest"],
    ["search quality", "inherits whatever your engines return, and says so", "tuned proprietary ranking"],
    ["pick it when", "privacy, zero ops beyond one process, you own the cache", "zero-ops by outsourcing, per-call quality"],
  ];

  const factRows: [string, string][] = [
    ["binary", "one ~37 MB binary, no runtime deps"],
    ["idle memory", "< 80 MB full mode · < 50 MB headless · < 40 MB mcp"],
    ["storage", "SQLite (WAL): search log, answers and cache, all on your disk"],
    ["license", "MPL-2.0 (code) · Apache-2.0 (engines, SDK)"],
  ];
</script>

<div class="landing">
  <section id="one-server">
    <h2>One server, three surfaces.</h2>
    <p class="lead">
      Same pipeline, same cache, same <code>search_log</code>. Pick the surface that fits.
    </p>
    <div class="bento">
      <div class="card ui">
        <h3>Web UI</h3>
        <p>
          A Svelte SPA served from the same binary: progressive results over SSE, an AI
          answer mode, history, admin and dark or light themes, on your own loopback.
        </p>
        <p class="mono small muted">{"GET /app/* · /answer/{id}"}</p>
      </div>
      <div class="card api">
        <h3>HTTP API</h3>
        <p>
          Every UI surface is a JSON route (search, answers, history, cache) with a
          per-request <code>request_id</code> you can trace end to end.
        </p>
        <Code code={apiSnippet} lang="shell" />
        <Code code={apiResponse} lang="json" />
      </div>
      <div class="card mcp">
        <h3>MCP server</h3>
        <p>
          Streamable HTTP MCP at <code>/mcp</code>. Point Claude Code, Hermes, or any MCP
          client at it and the agent gets web search tools.
        </p>
        <Code code={mcpConfig} lang="json" />
      </div>
    </div>
  </section>

  <section>
    <h2>Answers that show their sources.</h2>
    <p class="lead">
      AI mode runs a grounded loop over your own metasearch results. Every claim carries
      an inline <code>[n]</code> citation back to a real result card, a confidence score,
      and an <em>ungrounded</em> flag when the evidence ran out.
    </p>
    <p>
      Each finished answer gets a durable URL (<code>/answer/1</code>, not a query string).
      It is shareable, linkable, and still there after the 24 h cache expires. Yours, in
      your history, on your disk.
    </p>
    <p class="mono small muted">POST /api/answer · SSE stream · OpenAI or Anthropic compatible providers</p>
  </section>

  <section>
    <h2>Engines are YAML, not code.</h2>
    <p class="lead">
      Brave, Bing, DuckDuckGo ship as declarative engine specs (request URL, CSS or
      JSONPath selectors, timeout) that a shared runtime fans out, merges with reciprocal
      rank fusion, and caches. Write your own the same way: scrape a site, call a keyed
      API, or replay fixtures offline.
    </p>
    <div class="grid-2">
      <Code code={engineYaml} lang="yaml" />
      <ul class="points">
        <li>
          <strong>Tail tolerant fan-out.</strong> Per-engine deadlines and hedging, not
          the slowest upstream.
        </li>
        <li>
          <strong>Shared TTL cache.</strong> Every surface hits the same cache, so repeat
          queries are free.
        </li>
        <li>
          <strong>Honest failure modes.</strong> Rate limited, blocked, and timed out
          engines degrade the result set, never the page.
        </li>
      </ul>
    </div>
  </section>

  <section>
    <h2>Self-hosted, not pay-per-call.</h2>
    <p class="lead">
      Hosted search APIs for agents (Tavily, Exa) sell you searches as a metered service. cauce
      is the other end of the trade: the same three surfaces, running where you run.
    </p>
    <div class="panel scroll-x">
      <table>
        <thead>
          <tr><th></th><th>cauce</th><th>hosted AI search APIs</th></tr>
        </thead>
        <tbody>
          {#each compareRows as [k, a, b] (k)}
            <tr><th>{k}</th><td>{a}</td><td>{b}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section>
    <h2>Self-host in one command.</h2>
    <p class="lead">
      No containers, no managed services, no account. The binary is the whole product.
      Run it on a laptop, a VPS, or an always-on box at home.
    </p>
    <div class="panel">
      <Code code={installSnippet} lang="shell" />
      <table>
        <tbody>
          {#each factRows as [k, v] (k)}
            <tr><th>{k}</th><td>{v}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <footer class="foot">
    <span class="muted">Local metasearch for humans and agents.</span>
    <a href="https://github.com/espetro/cauce">GitHub</a>
    <a href="https://github.com/espetro/cauce/releases">Releases</a>
    <a href="https://docs.cauce.fyi">Docs</a>
    <a href="https://github.com/espetro/cauce/blob/main/LICENSE">MPL-2.0</a>
  </footer>
</div>

<style>
  /* Styles live in Landing.css, imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./Landing.css";
</style>
