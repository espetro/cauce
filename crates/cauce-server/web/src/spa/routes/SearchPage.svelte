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
  import UiButton from "../ui/button.svelte";
  import UiCollapsible from "../ui/collapsible.svelte";

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
        {#if page.metaDetail}
          <UiCollapsible>
            {#snippet trigger(open)}
              <span class="meta-detail-summary">
                <span class="meta-detail-chevron" class:open aria-hidden="true">▸</span
                >{spa.app.cache_details}
              </span>
            {/snippet}
            <span class="meta-detail-body">{page.metaDetail}</span>
          </UiCollapsible>
        {/if}
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
        <div class="more-row">
          <UiButton disabled={page.moreBusy} onclick={() => page.loadMore()}
            >{spa.search.more}</UiButton
          >
        </div>
      {/if}
    {/if}
  {/if}
</main>

<style>
  .more-row {
    margin: 0.75rem 0;
  }

  /* The former details/summary disclosure look: a quiet chevron
     that rotates open, label unchanged. */
  .meta-detail-summary {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  .meta-detail-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }

  .meta-detail-chevron.open {
    transform: rotate(90deg);
  }

  .meta-detail-body {
    display: inline-block;
    padding-left: 1rem;
  }
</style>
