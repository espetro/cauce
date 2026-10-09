<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin` — plan §7.4 merged ops surface: one route, three tabs
  (`engines`, `cache`, `audit`), each a self-contained feature slice.
  Tab links are real `<a href="/app/admin?tab=…">` so the URL stays
  shareable and the hash is free for engine-card anchors.
-->
<script lang="ts">
  import { spa } from "../../lib/i18n.js";
  import AuditTab from "./AuditTab.svelte";
  import CacheTab from "./CacheTab.svelte";
  import EnginesTab from "./EnginesTab.svelte";
  import type { createAdminPage } from "./admin.svelte.js";

  interface AdminViewProps {
    page: ReturnType<typeof createAdminPage>;
  }

  let { page }: AdminViewProps = $props();
  const s = $derived(page.state);
</script>

<h1>{spa.admin.title}</h1>
<nav class="tabs" aria-label={spa.admin.tabs_label}>
  {#each page.TABS as t}
    <a href={page.href(t)} class="tab" class:active={s.tab === t}
      >{spa.admin["tab_" + t as keyof typeof spa.admin]}</a
    >
  {/each}
</nav>

{#if s.tab === "engines"}
  <EnginesTab tab={page.engines} />
{:else if s.tab === "cache"}
  <CacheTab tab={page.cache} />
{:else}
  <AuditTab tab={page.audit} />
{/if}

<style>
  .tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--border);
    margin-bottom: 1rem;
  }
  .tab {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    text-decoration: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tab.active {
    color: var(--fg);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
</style>
