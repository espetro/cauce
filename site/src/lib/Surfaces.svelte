<!-- MPL-2.0 -->
<script lang="ts">
  import CodeBlock from './CodeBlock.svelte';

  const apiSnippet = `# the same search the UI runs, as JSON
curl 'http://127.0.0.1:4479/api/search?q=reciprocal+rank+fusion'`;

  const apiResponse = `{
  "results": [ { "url": "…", "title": "…", "engine": "brave" } ],
  "meta": { "source": "network", "elapsed_ms": 348,
            "request_id": "01a11d4e-…" }
}`;

  const mcpConfig = `// ~/.claude/settings.json — four tools, streamable HTTP
{
  "mcpServers": {
    "search": { "type": "http",
                "url": "https://search.localhost/mcp" }
  }
}
// tools: search_web · exa_search · cache_status · cache_invalidate`;
</script>

<section>
  <div class="wrap">
    <h2>One server, three surfaces.</h2>
    <p class="muted lead">
      Same pipeline, same cache, same <code>search_log</code> — pick the surface
      that fits.
    </p>

    <div class="grid">
      <div class="card">
        <h3>Web UI</h3>
        <p class="muted">
          A server-rendered search page with progressive results over SSE, an
          answer mode, history, and dark/light themes — on your own loopback.
        </p>
        <p class="mono small muted">GET / · /search · /answer · /history</p>
      </div>

      <div class="card">
        <h3>HTTP API</h3>
        <p class="muted">
          Every UI surface is a JSON route — search, answers, history, cache —
          with a per-request <code>request_id</code> you can trace end to end.
        </p>
        <CodeBlock code={apiSnippet} lang="bash" />
        <CodeBlock code={apiResponse} lang="json" />
      </div>

      <div class="card">
        <h3>MCP server</h3>
        <p class="muted">
          Streamable-HTTP MCP at <code>/mcp</code> — point Claude Code, Hermes,
          or any MCP client at it and the agent gets web search tools.
        </p>
        <CodeBlock code={mcpConfig} lang="json" />
      </div>
    </div>
  </div>
</section>

<style>
  h2 { font-size: clamp(26px, 4vw, 36px); margin: 0 0 12px; }
  .lead { margin: 0 0 36px; max-width: 560px; font-size: 17px; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 20px;
    align-items: start;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 24px;
    background: var(--card-bg);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card h3 { margin: 0; font-size: 19px; }
  .card p { margin: 0; }
  .small { font-size: 13px; }
</style>
