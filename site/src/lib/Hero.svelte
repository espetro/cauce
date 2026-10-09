<!-- MPL-2.0 -->
<script lang="ts">
  import searchDark from '../assets/search-dark.png';
  import searchLight from '../assets/search-light.png';

  // Set VITE_CAUCE_ORIGIN at build time to point the demo search box at the
  // public instance (e.g. https://search.example.com). When unset the form
  // submits to /app/search on whatever origin serves this page — which is a
  // real cauce search when the binary serves the site itself.
  const origin = (import.meta.env.VITE_CAUCE_ORIGIN ?? '').replace(/\/$/, '');

  let q = $state('');
</script>

<section class="hero">
  <div class="wrap">
    <h1>Local metasearch<br />for humans and agents.</h1>
    <p class="sub">
      One binary serving a web UI, an HTTP API, and an MCP server — with a
      shared cache and grounded AI answers.
    </p>

    <form class="search" action="{origin}/app/search" method="get" target="_blank" rel="noopener">
      <input
        type="search"
        name="q"
        bind:value={q}
        placeholder="Search the web…"
        autocomplete="off"
        spellcheck="false"
        aria-label="Search the web"
      />
      <button type="submit">Search</button>
    </form>
    <p class="hint muted">
      {#if origin}
        Opens {origin} in a new tab.
      {:else}
        Points at the cauce instance serving this page — or self-host in one
        command below.
      {/if}
    </p>

    <div class="shot">
      <img
        src={searchDark}
        alt="cauce search results for 'what is reciprocal rank fusion' — 30 results, live, 372 ms, bing and brave engines"
        loading="eager"
      />
      <img
        class="light-only"
        src={searchLight}
        alt=""
        aria-hidden="true"
        loading="lazy"
      />
    </div>
  </div>
</section>

<style>
  .hero { padding-top: 72px; }
  h1 {
    font-size: clamp(34px, 5.4vw, 52px);
    margin: 0 0 16px;
    max-width: 640px;
  }
  .sub {
    margin: 0 0 28px;
    font-size: 18px;
    color: var(--muted);
    max-width: 560px;
  }
  .search {
    display: flex;
    gap: 8px;
    max-width: 560px;
  }
  .search input {
    flex: 1;
    font: inherit;
    font-size: 17px;
    padding: 11px 14px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .search input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .search button {
    font: inherit;
    font-size: 16px;
    padding: 11px 22px;
    color: #fff;
    background: var(--accent);
    border: 0;
    border-radius: 8px;
    cursor: pointer;
  }
  .search button:hover { filter: brightness(1.08); }
  .search button:active { transform: translateY(1px); }
  .hint { font-size: 13px; margin: 10px 0 0; }
  .shot {
    margin-top: 48px;
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.12);
  }
  .shot img { display: block; width: 100%; }
  .shot .light-only { display: none; }
  @media (prefers-color-scheme: light) {
    .shot img:first-child { display: none; }
    .shot .light-only { display: block; }
  }
</style>
