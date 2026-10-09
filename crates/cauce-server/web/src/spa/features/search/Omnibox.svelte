<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The variant-E composer (§7.1): a box, not a pill — an input row
  (query + send) and a tool row (the Search/✦Ask mode segment, the
  `ai_mode` pill's composer form). `compact` is the /search shape;
  landing renders it full-size. Submit delegates to the owning page
  (navigate for search, `/answer?q=` handoff in AI mode).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../../lib/i18n.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";

  interface OmniboxProps {
    value?: string;
    aiMode?: boolean;
    compact?: boolean;
    autofocus?: boolean;
    onsubmit: () => void;
  }

  let {
    value = $bindable(""),
    aiMode = $bindable(false),
    compact = false,
    autofocus = false,
    onsubmit,
  }: OmniboxProps = $props();

  let inputEl = $state<HTMLInputElement>();

  onMount(() => {
    if (autofocus) inputEl?.focus();
  });

  function arm(mode: "search" | "ai") {
    aiMode = mode === "ai";
    inputEl?.focus();
  }
</script>

<form
  class="composer"
  class:compact
  onsubmit={(e) => {
    e.preventDefault();
    onsubmit();
  }}
>
  <div class="input-row">
    <input
      bind:this={inputEl}
      type="search"
      name="q"
      bind:value
      placeholder={aiMode ? spa.answer.placeholder : spa.search.placeholder}
      aria-label={aiMode ? spa.answer.placeholder : spa.search.placeholder}
      autocomplete="off"
    />
    <button type="submit" class="send">{aiMode ? spa.answer.submit : spa.search.submit}</button>
  </div>
  {#if capabilities.aiEnabled}
    <div class="tool-row">
      <div class="segment" role="group">
        <button
          type="button"
          aria-pressed={!aiMode}
          onclick={() => arm("search")}>{spa.search.submit}</button
        >
        <button
          type="button"
          aria-pressed={aiMode}
          data-placeholder={spa.answer.placeholder}
          data-submit={spa.answer.submit}
          onclick={() => arm("ai")}>✦ {spa.search.ai_mode}</button
        >
      </div>
    </div>
  {/if}
</form>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin-bottom: 1rem;
    padding: 0.5rem;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    background: var(--bg);
  }

  .composer:focus-within {
    border-color: var(--accent);
  }

  .composer.compact {
    padding: 0.375rem;
    margin-bottom: 0.75rem;
  }

  .input-row {
    display: flex;
    gap: 0.375rem;
    align-items: center;
    min-width: 0;
  }

  input[type="search"] {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.5rem;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--fg);
    font-size: 1rem;
  }

  input[type="search"]:focus {
    outline: none;
  }

  .send {
    flex: none;
    padding: 0.5rem 1rem;
    border: none;
    border-radius: var(--radius);
    background: var(--accent);
    color: #fff;
    font-size: 1rem;
    cursor: pointer;
  }

  .compact .send {
    padding: 0.375rem 0.875rem;
  }

  .tool-row {
    display: flex;
    align-items: center;
    padding: 0 0.125rem;
  }

  .segment {
    display: inline-flex;
    gap: 0.125rem;
    padding: 0.125rem;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  .segment button {
    padding: 0.25rem 0.75rem;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font-size: 0.8125rem;
    cursor: pointer;
  }

  .segment button[aria-pressed="true"] {
    background: var(--accent);
    color: #fff;
  }
</style>
