<!-- MPL-2.0 -->
<script lang="ts">
  const links = [
    { href: 'https://github.com/espetro/cauce', label: 'github' },
    { href: 'https://github.com/espetro/cauce/tree/main/docs', label: 'docs' },
    { href: 'https://github.com/espetro/cauce/releases', label: 'releases' },
  ];
</script>

<nav>
  <div class="wrap nav-inner">
    <a class="wordmark" href="/">cauce</a>
    <div class="links">
      {#each links as l (l.href)}
        <a href={l.href} target="_blank" rel="noopener">{l.label}</a>
      {/each}
    </div>
  </div>
</nav>

<style>
  nav {
    border-bottom: 1px solid var(--line);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 10;
  }
  .nav-inner {
    display: flex;
    align-items: center;
    height: 60px;
    gap: 24px;
  }
  .wordmark {
    font-weight: 700;
    font-size: 18px;
    color: var(--fg);
    letter-spacing: -0.03em;
  }
  .wordmark:hover { text-decoration: none; }
  .links {
    display: flex;
    gap: 20px;
    font-size: 14px;
  }
  .links a { color: var(--muted); }
  .links a:hover { color: var(--fg); text-decoration: none; }
</style>
