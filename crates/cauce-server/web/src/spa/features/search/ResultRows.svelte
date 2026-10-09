<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  Result rows: favicon + linked title + host + snippet, styled by the
  shared `.rows` hairline rules in app.css (variant E). Clicks route to
  the page state, which fires the `/api/click` (and, when armed,
  `/api/pages`) beacons — the anchors stay normal `target="_blank"`
  navigations like the SSR rows.
-->
<script lang="ts">
  import { faviconUrl } from "../../lib/format.js";
  import type { SearchRow } from "./search.svelte.js";

  interface ResultRowsProps {
    rows: SearchRow[];
    emptyText: string;
    onclick: (row: SearchRow) => void;
  }

  let { rows, emptyText, onclick }: ResultRowsProps = $props();
</script>

<div id="results" class="rows" aria-live="polite">
  {#each rows as row (row.key)}
    {#if !row.hidden}
      <article data-key={row.key}>
        {#if row.host}
          <img src={faviconUrl(row.host)} width="16" height="16" alt="" loading="lazy" />
        {/if}
        <a href={row.url} target="_blank" rel="noopener" onclick={() => onclick(row)}
          >{row.title}</a
        >
        {#if row.host}
          <span class="host">{row.host}</span>
        {/if}
        <p class="snippet">{row.snippet}</p>
      </article>
    {/if}
  {/each}
  {#if emptyText}
    <p>{emptyText}</p>
  {/if}
</div>
