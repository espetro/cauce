<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/search` — the §7.3 route shell: reads the route params, owns the
  feature state, composes the omnibox + meta line + assist + rows. The
  App's `{#key}` remounts it per navigation, so `run` fires exactly once
  per URL (the SSR page's per-request semantics).
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { appHref, navigate } from "../app/router.svelte.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { spa } from "../lib/i18n.js";
  import { createSearchPage } from "../features/search/search.svelte.js";
  import Omnibox from "../features/search/Omnibox.svelte";
  import ResultRows from "../features/search/ResultRows.svelte";
  import AssistCard from "../features/answer/AssistCard.svelte";

  interface SearchPageProps {
    params: URLSearchParams;
  }

  let { params }: SearchPageProps = $props();

  const page = createSearchPage();

  const askUrl = $derived(
    capabilities.aiEnabled ? appHref("/answer?q=" + encodeURIComponent(page.q)) : "",
  );

  onMount(() => {
    void page.run(params);
    document.title = page.q ? page.q + " · " + spa.common.brand : spa.common.brand;
  });

  onDestroy(() => page.dispose());
</script>

<main>
  <h1 class="vh">{page.q || spa.common.brand}</h1>
  <Omnibox
    bind:value={page.q}
    bind:aiMode={page.aiMode}
    compact
    onsubmit={() => page.submit(navigate)}
  />
  {#if page.hasSearched}
    {#if page.errorText}
      <p class="field-error" role="alert">{page.errorText}</p>
    {:else}
      <div class="meta">
        {#if page.countText}<span id="result-count">{page.countText}</span>{/if}
        {#if page.metaText}<span id="search-meta">{page.metaText}</span>{/if}
        {#if page.requestId}
          <span id="request-id" class="request-id" title={page.requestIdFull}>{page.requestId}</span>
        {/if}
        {#if askUrl}<span><a id="ask-link" href={askUrl}>{spa.answer.ask_link}</a></span>{/if}
      </div>
      {#if page.statusText}
        <div id="search-stream" aria-live="polite" aria-busy={page.streaming}>
          <span id="stream-status" role="status">{page.statusText}</span>
        </div>
      {/if}
      {#if page.newAbove}
        <p id="new-results-above" class="new-results-above" role="status">{page.newAbove}</p>
      {/if}
      {#if capabilities.aiEnabled}
        <AssistCard
          q={page.q}
          context={page.assistContext}
          {askUrl}
          disabled={page.streaming}
        />
      {/if}
      <ResultRows rows={page.rows} emptyText={page.emptyText} onclick={(r) => page.clickRow(r)} />
      {#if page.moreVisible}
        <button
          type="button"
          class="more-btn"
          disabled={page.moreBusy}
          onclick={() => page.loadMore()}>{spa.search.more}</button
        >
      {/if}
    {/if}
  {/if}
</main>

<style>
  .more-btn {
    margin: 0.75rem 0;
    padding: 0.5rem 1rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.875rem;
    cursor: pointer;
  }

  .more-btn:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .more-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
