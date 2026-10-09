<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=cache` — one-for-one with `templates/cache.html`:
  `q` filter form, count + filtered-cap line, expired/all bulk delete,
  `UiCollapsible` rows with lazy pretty-JSON payload, row delete, pager.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiCollapsible from "../../ui/collapsible.svelte";
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
  <UiButton type="submit">{c.filter_button}</UiButton>
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
      <UiButton size="sm" onclick={() => tab.bulk("expired")}
        >{c.delete_expired}</UiButton
      >
      <UiButton size="sm" variant="danger" onclick={() => tab.bulk("all")}
        >{c.delete_all}</UiButton
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
          <div class="cache-disclosure">
            <UiCollapsible
              id={row.key}
              onOpenChange={(open) => {
                if (open) tab.loadPayload(row);
              }}
            >
              {#snippet trigger(open)}
                <span class="cache-chevron" class:open aria-hidden="true"
                  >▸</span
                >
                <span class="cache-query">{row.query}</span>
                <span class="cache-meta">
                  <span>{row.created}</span>
                  <span class="cache-expires">{row.expires}</span>
                  <span>{row.hitsLabel}</span>
                  <span>{row.engines}</span>
                  <span>{row.size}</span>
                </span>
              {/snippet}
              <div class="cache-payload">
                {#if row.payloadLoading}
                  {c.payload_loading}
                {:else if row.payloadError !== ""}
                  <span class="row-error">{row.payloadError}</span>
                {:else if row.payload !== ""}
                  <pre class="code-view">{row.payload}</pre>
                {/if}
              </div>
            </UiCollapsible>
          </div>
          <UiButton
            size="sm"
            variant="danger"
            onclick={() => tab.remove(row)}>{c.delete_row}</UiButton
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
  .cache-disclosure {
    flex: 1 1 auto;
    min-width: 0;
  }
  /* The trigger is the row's flex-wrap container (was the disclosure
     summary); the wrapper's own recipe supplies flex/cursor/font — this
     only restores the wrap + baseline alignment. */
  .cache-disclosure :global(.ui-collapsible-trigger) {
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.75rem;
    min-width: 0;
  }
  .cache-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }
  .cache-chevron.open {
    transform: rotate(90deg);
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
  /* Row delete keeps the old flex-shrink so it never wraps under the
     disclosure column. */
  .cache-row :global(.ui-button) {
    flex-shrink: 0;
  }
</style>
