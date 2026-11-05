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
  import FilterBar from "../features/search/FilterBar.svelte";
  import UiButton from "../ui/button.svelte";

  interface SearchPageProps {
    params: URLSearchParams;
  }

  let { params }: SearchPageProps = $props();

  const page = createSearchPage();

  const askUrl = $derived(
    capabilities.aiEnabled ? appHref("/answer?q=" + encodeURIComponent(page.q)) : "",
  );

  // D3: the shared FilterBar's field array — pre-shaped per the honest
  // capability gate so the row never advertises an engine filter the
  // selected engines cannot honour (plan §182-188).
  const filterFields = $derived([
    {
      key: "time_range",
      label: spa.filter.time_range,
      type: "select" as const,
      value: page.timeRange,
      options: [
        { value: "", label: spa.filter.time_range_all },
        { value: "day", label: spa.filter.time_range_day },
        { value: "week", label: spa.filter.time_range_week },
        { value: "month", label: spa.filter.time_range_month },
        { value: "year", label: spa.filter.time_range_year },
      ],
    },
    {
      key: "safesearch",
      label: spa.filter.safesearch,
      type: "select" as const,
      value: page.safeSearch,
      options: [
        { value: "", label: spa.filter.safesearch_off },
        { value: "moderate", label: spa.filter.safesearch_moderate },
        { value: "strict", label: spa.filter.safesearch_strict },
      ],
    },
    {
      key: "engines",
      label: spa.filter.engines,
      type: "multi-select" as const,
      value: page.engines,
      options: capabilities.engineIds.map((id) => ({ value: id, label: id })),
      placeholder: spa.filter.engines_placeholder,
      hint: spa.filter.engines_hint,
      disabled: capabilities.engineIds.length <= 1,
    },
    {
      key: "lang",
      label: spa.filter.lang,
      type: "text" as const,
      value: page.lang,
      placeholder: spa.filter.lang_placeholder,
      submitOnChange: false,
    },
  ]);

  onMount(() => {
    void page.run(params);
    document.title = page.q ? page.q + " · " + spa.common.brand : spa.common.brand;
  });

  onDestroy(() => page.dispose());
</script>

<!-- DS-20B: no `prompt` here. The shell's prompt-card echoed the query
     in a box right above a composer that already held the same text —
     two identical "rust" boxes read as "did my search double-fire?".
     The card belongs to the answer thread (where the composer becomes
     a follow-up); search carries its context in the meta line. -->
<ConversationShell heading={page.q || spa.common.brand}>
  {#snippet composer()}
    <Omnibox
      bind:value={page.q}
      bind:aiMode={page.aiMode}
      compact
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
      <FilterBar
        fields={filterFields}
        onChange={(key, value) => page.setFilter(key, value)}
        showSubmit
        submitLabel={spa.filter.submit}
        onSubmit={() => {}}
      />
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
  /* Styles live in SearchPage.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./SearchPage.css";
</style>
