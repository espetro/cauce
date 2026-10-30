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
  .sources-panel {
    position: fixed;
    z-index: 5;
    top: 3.25rem;
    right: 0;
    bottom: 0;
    width: min(24rem, 40vw);
    overflow-y: auto;
    padding: 0.75rem 1rem;
    background: var(--bg);
    border-left: 1px solid var(--border);
    outline: none;
  }

  .sources-handle {
    display: none;
  }

  .sources-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .sources-head h2 {
    margin: 0;
    font-size: 0.9375rem;
  }

  .sources-list {
    list-style: none;
    margin: 0.5rem 0 0;
    padding: 0;
  }

  .source-row {
    display: flex;
    gap: 0.5rem;
    padding: 0.625rem 0;
    border-top: 1px solid var(--border);
  }

  .source-host {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    margin: 0;
    color: var(--muted);
    font-size: 0.8125rem;
  }

  .snippet {
    margin: 0.2rem 0 0;
    color: var(--muted);
    font-size: 0.875rem;
  }

  /* Bottom drawer: ≤80vh, bottom edge flush so the page's sticky
     composer (z-index 1) stays reachable above/behind via page scroll. */
  @media (width < 700px) {
    .sources-panel {
      top: auto;
      left: 0;
      width: auto;
      max-height: 80vh;
      border-left: none;
      border-top: 1px solid var(--border);
      border-radius: 0.75rem 0.75rem 0 0;
    }

    .sources-handle {
      display: block;
      width: 2.5rem;
      height: 0.3rem;
      margin: 0 auto 0.5rem;
      border-radius: 0.2rem;
      background: var(--border);
      cursor: grab;
      touch-action: none;
    }
  }
</style>
