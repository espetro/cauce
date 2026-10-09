<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  One thread turn (§7.1): query bubble, meta row (status / retrieval
  path / confidence / ungrounded / cached / model / request-id), the
  step lines collapsed into a quiet `<details>` by default (§7.2), the
  answer body (pre-wrap deltas while streaming, server-sanitized `.md`
  once `done.html` lands), then — only at `done` — the numbered source
  cards and related questions. `edit` shows on the last settled turn
  and rewinds it into the composer.
-->
<script lang="ts">
  import SourceCard from "./SourceCard.svelte";
  import { SA, fmt } from "../../lib/i18n.js";
  import type { AnswerTurnState } from "./thread.svelte.js";

  interface AnswerTurnProps {
    turn: AnswerTurnState;
    /** True on the last turn once settled and not busy — the edit affordance. */
    editable: boolean;
    onedit: () => void;
    /** Builds the href for a related-question link (routes inject the mapper). */
    relatedHref: (q: string) => string;
  }

  let { turn, editable, onedit, relatedHref }: AnswerTurnProps = $props();

  let textEl = $state<HTMLElement>();

  // #226: `done.html` is server-rendered + sanitized — inject it, then
  // retarget the `<a class="cite" data-cite="n">` placeholders to this
  // turn's numbered cards (`#src-<turn>-<n>`).
  $effect(() => {
    if (!turn.html || !textEl) return;
    for (const a of textEl.querySelectorAll<HTMLAnchorElement>("a.cite[data-cite]")) {
      const n = a.getAttribute("data-cite");
      if (n) a.setAttribute("href", `#src-${turn.n}-${n}`);
    }
  });
</script>

<article class="answer-turn" id="turn-{turn.n}">
  <p class="turn-q">
    {turn.q}
    {#if editable}
      <button type="button" class="turn-edit" onclick={onedit}>{SA.edit}</button>
    {/if}
  </p>
  <p class="meta">
    <span class="answer-status">{turn.status}</span>
    {#if turn.pathText}
      <span class="meta-chip answer-path" data-path={turn.pathKind}>{turn.pathText}</span>
    {/if}
    {#if turn.confidence !== null}
      <span class="meta-chip answer-confidence" data-confidence={turn.confidence}
        >{fmt(SA.confidence, { n: turn.confidence })}</span
      >
    {/if}
    {#if turn.ungrounded}
      <span class="meta-chip warn answer-ungrounded-badge">{SA.ungrounded_badge}</span>
    {/if}
    {#if turn.cached}
      <span class="meta-chip">{SA.cached}</span>
    {/if}
    {#if turn.model}
      <span class="meta-chip answer-meta">{turn.model}</span>
    {/if}
    {#if turn.requestId}
      <span class="request-id" title={turn.requestId}>{turn.requestId.slice(0, 8)}</span>
    {/if}
  </p>
  {#if turn.steps.length}
    <details class="answer-steps">
      <summary>{fmt(SA.steps, { n: turn.steps.length })}</summary>
      <ol>
        {#each turn.steps as step}
          <li>{step}</li>
        {/each}
      </ol>
    </details>
  {/if}
  {#if turn.ungrounded}
    <p class="ungrounded">{SA.ungrounded}</p>
  {/if}
  {#if turn.html || turn.deltas}
    <div class="answer-text" class:md={turn.html.length > 0} bind:this={textEl}>
      {#if turn.html}{@html turn.html}{:else}{turn.deltas}{/if}
    </div>
  {/if}
  {#if turn.sources.length}
    <section class="answer-sources">
      <h3 class="section-label">{SA.sources}</h3>
      <div class="sources-row">
        {#each turn.sources as source, i (source.url + i)}
          <SourceCard {source} n={i + 1} turn={turn.n} />
        {/each}
      </div>
    </section>
  {/if}
  {#if turn.related.length}
    <div class="answer-related">
      <span class="section-label">{SA.related}</span>
      {#each turn.related as r}
        <a class="related-question" href={relatedHref(r)}>{r}</a>
      {/each}
    </div>
  {/if}
  {#if turn.error}
    <p class="answer-error field-error" role="alert">{turn.error}</p>
  {/if}
</article>
