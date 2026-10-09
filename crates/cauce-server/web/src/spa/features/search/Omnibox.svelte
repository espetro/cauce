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
  import UiButton from "../../ui/button.svelte";
  import UiToggleGroup from "../../ui/toggle-group.svelte";

  interface OmniboxProps {
    value?: string;
    aiMode?: boolean;
    compact?: boolean;
    autofocus?: boolean;
    onsubmit: () => void;
  }

  interface ModeOption {
    value: "search" | "ai";
    label: string;
  }

  let {
    value = $bindable(""),
    aiMode = $bindable(false),
    compact = false,
    autofocus = false,
    onsubmit,
  }: OmniboxProps = $props();

  let inputEl = $state<HTMLInputElement>();

  const modeOptions = $derived<ModeOption[]>([
    { value: "search", label: spa.search.submit },
    { value: "ai", label: "✦ " + spa.search.ai_mode },
  ]);

  onMount(() => {
    if (autofocus) inputEl?.focus();
  });

  function arm(mode: "search" | "ai") {
    aiMode = mode === "ai";
    inputEl?.focus();
  }

  // ToggleGroup single can deselect to "" on re-press; the mode segment
  // must always resolve to one of the two modes, so ignore the empty value.
  function armFromGroup(mode: string) {
    if (mode === "search" || mode === "ai") arm(mode);
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
    <UiButton
      type="submit"
      variant="primary"
      size="icon"
      disabled={!value.trim()}
      ariaLabel={aiMode ? spa.answer.submit : spa.search.submit}
      ><span aria-hidden="true" class="send-icon">↑</span></UiButton
    >
  </div>
  {#if capabilities.aiEnabled}
    <div class="tool-row">
      <UiToggleGroup
        value={aiMode ? "ai" : "search"}
        options={modeOptions}
        onValueChange={armFromGroup}
      />
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

  /* ui/button's icon size is a fixed square; the glyph sizes itself. */
  .send-icon {
    font-size: 1rem;
    line-height: 1;
  }

  .tool-row {
    display: flex;
    align-items: center;
    padding: 0 0.125rem;
  }
</style>
