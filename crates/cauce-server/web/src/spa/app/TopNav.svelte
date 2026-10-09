<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The variant-E sticky chrome (§7.1): same header on every surface —
  brand + primary nav left, quiet operator group + settings + theme
  toggle right, operator links collapsing into the `more` <details>
  below 700px, all one-for-one with `templates/header.html`. Links that
  have no SPA twin yet stay plain `/...` hrefs (full load into the HTMX
  page); `/app`-bound links route client-side via App's delegated click.
-->
<script lang="ts">
  import { spa } from "../lib/i18n.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { appHref } from "./router.svelte.js";
  import { cycleTheme, themeAria, themeWord } from "./theme.svelte.js";

  let { active = "" }: { active?: string } = $props();

  const c = spa.common;
</script>

<header class="site">
  <a class="brand" href={appHref("/")}>{c.brand}</a>
  <nav class="nav-primary" aria-label={c.nav_primary_label}>
    <a href={appHref("/")} aria-current={active === "search" ? "page" : undefined}
      >{c.nav_search}</a
    >
    {#if capabilities.aiEnabled}
      <a href={appHref("/answer")} aria-current={active === "answer" ? "page" : undefined}
        >{c.nav_answer}</a
      >
    {/if}
    <a href="/history">{c.nav_history}</a>
    <a href="/dashboard">{c.nav_dashboard}</a>
    <a href="/archive">{c.nav_archive}</a>
  </nav>
  <div class="nav-right">
    <nav class="nav-operator" aria-label={c.nav_operator_label}>
      <a href="/engines">{c.nav_engines}</a>
      <a href="/cache">{c.nav_cache}</a>
      <a href="/audit">{c.nav_audit}</a>
    </nav>
    <a href="/settings">{c.nav_settings}</a>
    <button
      type="button"
      id="theme-toggle"
      aria-label={themeAria()}
      title={c.theme_switch}
      onclick={cycleTheme}>{themeWord()}</button
    >
    <details class="nav-more">
      <summary>{c.nav_more}</summary>
      <nav class="nav-more-links" aria-label={c.nav_operator_label}>
        <a href="/engines">{c.nav_engines}</a>
        <a href="/cache">{c.nav_cache}</a>
        <a href="/audit">{c.nav_audit}</a>
      </nav>
    </details>
  </div>
</header>

<style>
  .site {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 1rem;
    margin-bottom: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    font-size: 0.9375rem;
    transition: background-color 160ms ease;
  }

  @media (prefers-reduced-motion: reduce) {
    .site {
      transition: none;
    }
  }

  .site a {
    color: var(--fg);
    text-decoration: none;
  }

  .site a:hover {
    text-decoration: underline;
  }

  .brand {
    font-weight: 700;
  }

  .nav-primary,
  .nav-right {
    display: flex;
    align-items: baseline;
    gap: 0.9rem;
  }

  .nav-right {
    margin-left: auto;
    color: var(--muted);
  }

  .nav-right a {
    color: var(--muted);
  }

  .site a[aria-current="page"] {
    color: var(--accent);
  }

  .nav-operator {
    display: none;
    align-items: baseline;
    gap: 0.9rem;
  }

  #theme-toggle {
    padding: 0.1rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--muted);
    font-size: 0.8125rem;
    cursor: pointer;
  }

  /* CSS-only collapse: operator links move into `more` below 700px. */
  .nav-more {
    position: relative;
  }

  .nav-more > summary {
    cursor: pointer;
    color: var(--muted);
    list-style: none;
  }

  .nav-more > summary::-webkit-details-marker {
    display: none;
  }

  .nav-more[open] > summary {
    color: var(--accent);
  }

  .nav-more-links {
    position: absolute;
    right: 0;
    top: 100%;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 6.5rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    box-shadow: 0 2px 8px rgb(0 0 0 / 15%);
  }

  @media (width >= 700px) {
    .nav-operator {
      display: flex;
    }

    .nav-more {
      display: none;
    }
  }
</style>
