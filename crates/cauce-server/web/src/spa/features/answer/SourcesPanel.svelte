<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The sources panel (UX-04): the selected turn's sources as numbered
  rows (favicon · domain · title · snippet). One component, two
  presentations by CSS: a right-side panel on desktop, a bottom drawer
  (≤80vh, drag the handle down to dismiss) under 700px. It is a
  non-modal `<aside>` — it never traps focus or covers the composer's
  keyboard path; Escape closes it and focus returns to the chip that
  opened it. The drag-to-dismiss handle is hand-rolled pointer
  handling: no bits-ui part models a drag sheet, so it falls under the
  migration's justified-exception rule. Rows keep the
  `src-<turn>-<n>` ids the answer body's citation anchors target.
-->
<script lang="ts">
  import { XIcon } from "phosphor-svelte";
  import { SA, fmt } from "../../lib/i18n.js";
  import { faviconUrl, hostOf } from "../../lib/format.js";
  import UiButton from "../../ui/button.svelte";
  import type { AnswerSource } from "../../../types/AnswerSource.js";

  interface SourcesPanelProps {
    /** 1-based turn the panel is scoped to. */
    turn: number;
    /** Total turns in the thread (the `N/M` in the header). */
    total: number;
    sources: AnswerSource[];
    onclose: () => void;
  }

  let { turn, total, sources, onclose }: SourcesPanelProps = $props();

  const DISMISS_PX = 80;

  let asideEl = $state<HTMLElement | null>(null);
  let dragStart: number | null = null;
  let dragY = $state(0);

  // Focus the panel on open so keyboard users land inside it.
  $effect(() => {
    asideEl?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      onclose();
    }
  }

  function dragBegin(event: PointerEvent): void {
    dragStart = event.clientY;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function dragMove(event: PointerEvent): void {
    if (dragStart === null) return;
    dragY = Math.max(0, event.clientY - dragStart);
  }

  function dragEnd(): void {
    if (dragStart === null) return;
    const dismiss = dragY > DISMISS_PX;
    dragStart = null;
    dragY = 0;
    if (dismiss) onclose();
  }
</script>

<svelte:window {onkeydown} />

<aside
  bind:this={asideEl}
  class="sources-panel"
  aria-label={fmt(SA.sources_title, { n: turn, m: total })}
  tabindex="-1"
  style:transform={dragY ? `translateY(${dragY}px)` : undefined}
>
  <div
    class="sources-handle"
    role="presentation"
    title={SA.sources_drag}
    onpointerdown={dragBegin}
    onpointermove={dragMove}
    onpointerup={dragEnd}
    onpointercancel={dragEnd}
  ></div>
  <header class="sources-head">
    <h2>{fmt(SA.sources_title, { n: turn, m: total })}</h2>
    <UiButton variant="ghost" size="icon" ariaLabel={SA.sources_close} onclick={onclose}
      ><XIcon size={15} aria-hidden="true" /></UiButton
    >
  </header>
  <ol class="sources-list">
    {#each sources as source, i (source.url + i)}
      {@const host = hostOf(source.url)}
      <li class="source-row" id="src-{turn}-{i + 1}">
        <span class="cite-badge">[{i + 1}]</span>
        <div>
          <p class="source-host">
            {#if host}<img src={faviconUrl(host)} width="16" height="16" alt="" loading="lazy" />{/if}
            {host}
          </p>
          {#if source.url}
            <a class="source-title" href={source.url} target="_blank" rel="noopener"
              >{source.title || host || source.url}</a
            >
          {:else}
            <span class="source-title">{source.title || host}</span>
          {/if}
          {#if source.snippet}<p class="snippet">{source.snippet}</p>{/if}
        </div>
      </li>
    {/each}
  </ol>
</aside>

<style>
  /* Styles live in SourcesPanel.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./SourcesPanel.css";
</style>
