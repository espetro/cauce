<!-- MPL-2.0 -->
<script lang="ts">
  import CodeBlock from './CodeBlock.svelte';

  const install = `# grab a release binary (linux x86_64 + aarch64, macOS arm64 + x86_64)
curl -LO https://github.com/espetro/cauce/releases/download/v0.8.1/\\
  cauce-v0.8.1-x86_64-unknown-linux-musl.tar.gz
tar -xzf cauce-v0.8.1-x86_64-unknown-linux-musl.tar.gz
./cauce serve          # UI + API + MCP on http://127.0.0.1:4479`;

  const cargo = `# or build from source
cargo install --locked --git https://github.com/espetro/cauce cauce-cli`;

  const rows = [
    ['binary', 'one ~37 MB binary, no runtime deps'],
    ['idle memory', '< 80 MB full mode · < 50 MB headless · < 40 MB mcp'],
    ['storage', 'SQLite (WAL) — search log, answers, cache, all on your disk'],
    ['license', 'MPL-2.0 (code) · Apache-2.0 (engines, SDK)'],
  ];
</script>

<section class="install" id="install">
  <div class="wrap">
    <h2>Self-host in one command.</h2>
    <p class="muted lead">
      No containers, no managed services, no account. The binary is the whole
      product — run it on a laptop, a VPS, or an always-on box at home.
    </p>
    <div class="cols">
      <CodeBlock code={install} lang="bash" />
      <CodeBlock code={cargo} lang="bash" />
    </div>
    <table>
      <tbody>
        {#each rows as [k, v] (k)}
          <tr><th class="mono">{k}</th><td class="muted">{v}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .install { border-top: 1px solid var(--line); }
  h2 { font-size: clamp(26px, 4vw, 36px); margin: 0 0 12px; }
  .lead { max-width: 560px; margin: 0 0 32px; font-size: 17px; }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    margin-bottom: 32px;
    align-items: start;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    border-top: 1px solid var(--line);
    font-size: 15px;
  }
  th, td { padding: 12px 16px; text-align: left; border-bottom: 1px solid var(--line); }
  th { font-size: 13px; color: var(--fg); font-weight: 500; white-space: nowrap; }
  @media (max-width: 820px) {
    .cols { grid-template-columns: 1fr; }
  }
</style>
