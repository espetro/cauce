<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/archive` markup — one-for-one with `templates/archive.html`:
  filter form, count line, UiCollapsible rows with lazy markdown, row
  delete, browse-mode pager, disabled notice when the fetch pipeline
  is down.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiCollapsible from "../../ui/collapsible.svelte";
  import type { createArchivePage } from "./archive.svelte.js";

  interface ArchiveViewProps {
    page: ReturnType<typeof createArchivePage>;
  }

  let { page }: ArchiveViewProps = $props();
  const a = spa.archive;
  const s = $derived(page.state);
</script>

<h1>{a.page_title}</h1>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.disabled}
  <p class="notice">
    {a.disabled} <a href="/app/settings">{a.disabled_link}</a>
  </p>
{:else}
  <form
    class="filters"
    action="/app/archive"
    method="get"
    onsubmit={(e) => {
      e.preventDefault();
      page.submit(navigate);
    }}
  >
    <input
      type="search"
      name="q"
      bind:value={s.q}
      placeholder={a.filter_placeholder}
    />
    <UiButton type="submit">{a.filter_button}</UiButton>
    {#if s.searching}
      <a href="/app/archive">{a.filter_clear}</a>
    {/if}
  </form>

  <div class="meta">
    <span id="archive-count">{s.countLine}</span>
  </div>

  {#if s.rows.length === 0}
    <p class="empty">{s.emptyLine}</p>
  {/if}

  {#each s.rows as row (row.url)}
    {#if !row.gone}
      <article class="archive-entry">
        <div class="archive-row">
          <div class="archive-disclosure">
            <UiCollapsible
              onOpenChange={(open) => {
                if (open) page.loadMarkdown(row);
              }}
            >
              {#snippet trigger(open)}
                <span class="archive-chevron" class:open aria-hidden="true"
                  >▸</span
                >
                <span class="archive-title">{row.title}</span>
                <span class="archive-meta">
                  <span>{row.host}</span>
                  <span>{row.fetched}</span>
                </span>
              {/snippet}
              {#if row.snippet !== ""}
                <p class="archive-snippet">{row.snippet}</p>
              {/if}
              <div class="archive-markdown">
                {#if row.markdownLoading}
                  {a.markdown_loading}
                {:else if row.markdownError !== ""}
                  <span class="row-error">{row.markdownError}</span>
                {:else if row.markdown !== ""}
                  <pre class="code-view">{row.markdown}</pre>
                {/if}
              </div>
            </UiCollapsible>
          </div>
          {#if page.canDelete()}
            <UiButton
              size="sm"
              variant="danger"
              onclick={() => page.remove(row)}>{a.delete_row}</UiButton
            >
          {/if}
        </div>
      </article>
    {/if}
  {/each}

  {#if !s.searching && (s.offset > 0 || s.hasMore)}
    <div class="pager meta">
      {#if s.offset > 0}
        <a href={page.pagerUrl(s.offset - page.LIMIT)}>&larr; {a.page_prev}</a>
      {/if}
      {#if s.hasMore}
        <a href={page.pagerUrl(s.offset + page.LIMIT)}>{a.page_next} &rarr;</a>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .archive-entry {
    border-bottom: 1px solid var(--border);
    padding: 0.4rem 0;
  }
  .archive-row {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    min-width: 0;
  }
  .archive-disclosure {
    flex: 1 1 auto;
    min-width: 0;
  }
  /* The trigger is the row's flex-wrap container (was the disclosure
     summary); the wrapper's own recipe supplies flex/cursor/font — this
     only restores the wrap + baseline alignment. */
  .archive-disclosure :global(.ui-collapsible-trigger) {
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.75rem;
    min-width: 0;
  }
  .archive-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }
  .archive-chevron.open {
    transform: rotate(90deg);
  }
  .archive-title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .archive-meta {
    color: var(--muted);
    font-size: 0.8125rem;
    display: inline-flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .archive-snippet {
    color: var(--muted);
    font-size: 0.875rem;
    margin: 0.25rem 0;
    overflow-wrap: anywhere;
  }
  /* Row delete keeps the old flex-shrink so it never wraps under the
     disclosure column. */
  .archive-row :global(.ui-button) {
    flex-shrink: 0;
  }
</style>
