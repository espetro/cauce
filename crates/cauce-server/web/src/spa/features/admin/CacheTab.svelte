<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=cache` — one-for-one with `templates/cache.html`:
  `q` filter form, count + filtered-cap line, expired/all bulk delete,
  `<details>` rows with lazy pretty-JSON payload, row delete, pager.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import type { createCacheTab } from "./cache.svelte.js";

  interface CacheTabProps {
    tab: ReturnType<typeof createCacheTab>;
  }

  let { tab }: CacheTabProps = $props();
  const c = spa.cache;
  const s = $derived(tab.state);
</script>

<form
  class="filters"
  onsubmit={(e) => {
    e.preventDefault();
    tab.submit(navigate);
  }}
>
  <input
    type="search"
    name="q"
    bind:value={s.q}
    placeholder={c.filter_placeholder}
  />
  <button type="submit">{c.filter_button}</button>
  {#if s.searching}
    <a href="/app/admin?tab=cache">{c.filter_clear}</a>
  {/if}
</form>

<div class="meta">
  <span id="cache-count">{s.countLine}</span>
  {#if s.searching && s.filteredCap !== ""}
    <span>{s.filteredCap}</span>
  {/if}
</div>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else}
  {#if s.rows.length > 0}
    <div class="cache-actions">
      <button type="button" onclick={() => tab.bulk("expired")}
        >{c.delete_expired}</button
      >
      <button type="button" class="danger" onclick={() => tab.bulk("all")}
        >{c.delete_all}</button
      >
      {#if s.bulkError}<span class="row-error">{s.bulkError}</span>{/if}
    </div>
  {/if}

  {#if s.rows.length === 0}
    <p class="empty">{s.emptyLine}</p>
  {/if}

  {#each s.rows as row (row.key)}
    {#if !row.gone}
      <article class="cache-entry" class:expired={row.expired}>
        <div class="cache-row">
          <details
            id={row.key}
            class="cache-details"
            ontoggle={(e) => {
              if ((e.target as HTMLDetailsElement).open)
                tab.loadPayload(row);
            }}
          >
            <summary>
              <span class="cache-query">{row.query}</span>
              <span class="cache-meta">
                <span>{row.created}</span>
                <span class="cache-expires">{row.expires}</span>
                <span>{row.hitsLabel}</span>
                <span>{row.engines}</span>
                <span>{row.size}</span>
              </span>
            </summary>
            <div class="cache-payload">
              {#if row.payloadLoading}
                {c.payload_loading}
              {:else if row.payloadError !== ""}
                <span class="row-error">{row.payloadError}</span>
              {:else if row.payload !== ""}
                <pre class="code-view">{row.payload}</pre>
              {/if}
            </div>
          </details>
          <button
            type="button"
            class="cache-delete"
            onclick={() => tab.remove(row)}>{c.delete_row}</button
          >
        </div>
      </article>
    {/if}
  {/each}

  {#if !s.searching && (s.offset > 0 || s.hasNext)}
    <div class="pager meta">
      {#if s.offset > 0}
        <a href={tab.pagerUrl(s.offset - 50)}>&larr; {c.page_prev}</a>
      {/if}
      {#if s.hasNext}
        <a href={tab.pagerUrl(s.offset + 50)}>{c.page_next} &rarr;</a>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .cache-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.75rem;
  }
  .cache-actions button {
    padding: 0.25rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.875rem;
    cursor: pointer;
  }
  .cache-actions button.danger {
    color: var(--warn);
  }
  .cache-entry {
    border-bottom: 1px solid var(--border);
    padding: 0.4rem 0;
  }
  .cache-entry.expired .cache-query {
    color: var(--muted);
  }
  .cache-row {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    min-width: 0;
  }
  .cache-details {
    flex: 1 1 auto;
    min-width: 0;
  }
  .cache-details summary {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.75rem;
    cursor: pointer;
    min-width: 0;
  }
  .cache-query {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .cache-meta {
    color: var(--muted);
    font-size: 0.8125rem;
    display: inline-flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .cache-expires {
    color: var(--muted);
  }
  .expired .cache-expires {
    color: var(--warn);
  }
  .cache-delete {
    flex-shrink: 0;
    padding: 0.15rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--warn);
    font-size: 0.8125rem;
    cursor: pointer;
  }
</style>
