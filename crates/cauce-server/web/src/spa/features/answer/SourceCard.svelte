<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  One numbered source card (HTMX `renderSources` parity): `id="src-<turn>-<n>"`
  is what the body's `a.cite[data-cite]` anchors retarget to. Honest
  states (§7.2): a missing title falls back to the host, an empty URL
  renders a non-linked card (no invented href), and a missing snippet
  leaves no ghost block. `AnswerSource` carries no freshness/confidence
  fields — the wire can't express stale/low-confidence cards, so none
  are faked.
-->
<script lang="ts">
  import { faviconUrl, hostOf } from "../../lib/format.js";
  import type { AnswerSource } from "../../../types/AnswerSource.js";

  interface SourceCardProps {
    source: AnswerSource;
    /** 1-based citation number (`data-cite` values key on it). */
    n: number;
    /** 1-based turn number — the id's `src-<turn>-<n>` prefix. */
    turn: number;
  }

  let { source, n, turn }: SourceCardProps = $props();

  const host = $derived(hostOf(source.url));
</script>

<article class="source-card" id="src-{turn}-{n}">
  <span class="cite-badge">[{n}]</span>
  {#if source.url}
    <a class="source-title" href={source.url} target="_blank" rel="noopener">
      {#if host}
        <img src={faviconUrl(host)} width="16" height="16" alt="" loading="lazy" />
      {/if}
      {source.title || host || source.url}
    </a>
  {:else}
    <span class="source-title">{source.title || host}</span>
  {/if}
  {#if host && source.url}
    <p class="host">{host}</p>
  {/if}
  {#if source.snippet}
    <p class="snippet">{source.snippet}</p>
  {/if}
</article>
