<!-- MPL-2.0 -->
<script lang="ts">
  import CodeBlock from './CodeBlock.svelte';

  const engineYaml = `# engines/brave.yaml (trimmed) — a search engine, as config
id: brave
tier: 1
page_size: 20
request:
  url: "https://search.brave.com/search?q={q}&offset={page0}&source=web"
  timeout_ms: 2500
parse:
  kind: html
  results: 'div.snippet[data-type="web"]'`;
</script>

<section class="engines">
  <div class="wrap">
    <h2>Engines are YAML, not code.</h2>
    <p class="muted lead">
      Brave, Bing, DuckDuckGo ship as declarative engine specs — request URL,
      CSS or JSONPath selectors, timeout — that a shared runtime fans out,
      merges with reciprocal rank fusion, and caches. Write your own the same
      way: scrape a site, call a keyed API, or replay fixtures offline.
    </p>
    <div class="cols">
      <CodeBlock code={engineYaml} lang="yaml" />
      <ul class="points">
        <li><strong>Tail-tolerant fan-out</strong> — per-engine deadlines and hedging, not the slowest upstream.</li>
        <li><strong>Shared TTL cache</strong> — every surface hits the same cache; repeat queries are free.</li>
        <li><strong>Honest failure modes</strong> — rate-limited, blocked, and timed-out engines degrade the result set, never the page.</li>
      </ul>
    </div>
  </div>
</section>

<style>
  .engines { border-top: 1px solid var(--line); }
  h2 { font-size: clamp(26px, 4vw, 36px); margin: 0 0 12px; }
  .lead { max-width: 640px; margin: 0 0 32px; font-size: 17px; }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 6fr) minmax(0, 5fr);
    gap: 40px;
    align-items: start;
  }
  .points { margin: 0; padding: 0; list-style: none; }
  .points li {
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
  }
  .points li:last-child { border-bottom: 0; }
  .points strong { color: var(--fg); }
  @media (max-width: 820px) {
    .cols { grid-template-columns: minmax(0, 1fr); gap: 28px; }
  }
</style>
