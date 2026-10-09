<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/archive` markup — one-for-one with `templates/archive.html`:
  filter form, count line, `<details>` rows with lazy markdown, row
  delete, browse-mode pager, disabled notice when the fetch pipeline
  is down.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
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
    <button type="submit">{a.filter_button}</button>
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
          <details
            class="archive-details"
            ontoggle={(e) => {
              if ((e.target as HTMLDetailsElement).open) page.loadMarkdown(row);
            }}
          >
            <summary>
              <span class="archive-title">{row.title}</span>
              <span class="archive-meta">
                <span>{row.host}</span>
                <span>{row.fetched}</span>
              </span>
            </summary>
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
          </details>
          <button class="archive-delete" onclick={() => page.remove(row)}>
            {a.delete_row}
          </button>
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
  .archive-details {
    flex: 1 1 auto;
    min-width: 0;
  }
  .archive-details summary {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.75rem;
    cursor: pointer;
    min-width: 0;
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
  .archive-delete {
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
