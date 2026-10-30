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
  import ConversationShell from "../app/ConversationShell.svelte";
  import Omnibox from "../features/search/Omnibox.svelte";
  import ResultRows from "../features/search/ResultRows.svelte";
  import UiButton from "../ui/button.svelte";

  interface SearchPageProps {
    params: URLSearchParams;
  }

  let { params }: SearchPageProps = $props();

  const page = createSearchPage();

  const askUrl = $derived(
    capabilities.aiEnabled && page.q.trim()
      ? appHref("/answer?q=" + encodeURIComponent(page.q.trim()))
      : "",
  );

  onMount(() => {
    void page.run(params);
    document.title = page.q ? page.q + " · " + spa.common.brand : spa.common.brand;
  });

  onDestroy(() => page.dispose());
</script>

<ConversationShell heading={page.q || spa.common.brand}>
  {#snippet composer()}
    <Omnibox
      bind:value={page.q}
      bind:aiMode={page.aiMode}
      compact
      onmodechange={(mode) => mode === "ai" && navigate(askUrl || appHref("/answer"))}
      onsubmit={() => page.submit(navigate)}
    />
  {/snippet}
  {#if page.hasSearched}
    {#if page.errorText}
      <p class="field-error" role="alert">{page.errorText}</p>
    {:else}
      <p class="meta search-meta">
        {#if page.countText}<span id="result-count">{page.countText}</span>{/if}
        {#if page.elapsedText}<span>{page.elapsedText}</span>{/if}
        {#if page.cacheText}<span id="search-meta">{page.cacheText}</span>{/if}
        {#if page.requestId}
          <span id="request-id" class="request-id" title={page.requestIdFull}>{page.requestId}</span>
        {/if}
        {#if askUrl}<span><a id="ask-link" href={askUrl}>{spa.answer.ask_link}</a></span>{/if}
      </p>
      {#if page.streamError}
        <p class="field-error" role="alert">{page.streamError}</p>
      {/if}
      {#if page.newAbove}
        <p id="new-results-above" class="new-results-above" role="status">{page.newAbove}</p>
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
</ConversationShell>

<style>
  .more-row {
    margin: 0.75rem 0;
  }
</style>
