<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The variant-E composer (§7.1), now on the `ai/prompt-input`
  foundation (DS-AI): `AiPromptInput` supplies the boxed composer +
  context (value flow, Enter-submit, autosize textarea), the tool row
  is `AiPromptActions` and the Search/✦Ask mode segment stays
  `UiToggleGroup` — the foundation ships no segmented control, so the
  ui/ wrapper remains the primitive. Send stays a UiButton icon.
  `compact` is the /search shape; landing renders it full-size.
  Submit delegates to the owning page (navigate for search,
  `/answer?q=` handoff in AI mode).
-->
<script lang="ts">
  import { ArrowUpIcon } from "phosphor-svelte";
  import { spa } from "../../lib/i18n.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import AiPromptInput from "../../ai/prompt-input/prompt-input.svelte";
  import AiPromptTextarea from "../../ai/prompt-input/prompt-input-textarea.svelte";
  import AiPromptActions from "../../ai/prompt-input/prompt-input-actions.svelte";
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

  // bind:ref into a $bindable prop throws props_invalid_value on
  // undefined — the ref must start as null, never undefined.
  let inputEl = $state<HTMLTextAreaElement | null>(null);

  const modeOptions = $derived<ModeOption[]>([
    { value: "search", label: spa.search.submit },
    { value: "ai", label: spa.search.ai_mode },
  ]);

  function arm(mode: "search" | "ai") {
    aiMode = mode === "ai";
    inputEl?.focus();
  }

  // ToggleGroup single writes "" when the pressed item is re-pressed; the
  // mode segment must always resolve to one of the two modes. The setter
  // ignores "", and because the binding is a getter/setter pair the group
  // keeps reading the armed mode instead of drifting to a local fallback.
  function armFromGroup(mode: string) {
    if (mode === "search" || mode === "ai") arm(mode);
  }
</script>

<form
  class="composer-form"
  onsubmit={(e) => {
    e.preventDefault();
    onsubmit();
  }}
>
  <AiPromptInput {value} onValueChange={(v) => (value = v)} onSubmit={onsubmit} {compact}>
    <div class="input-row">
      <AiPromptTextarea
        bind:ref={inputEl}
        {autofocus}
        name="q"
        autocomplete="off"
        placeholder={aiMode ? spa.answer.placeholder : spa.search.placeholder}
        ariaLabel={aiMode ? spa.answer.placeholder : spa.search.placeholder}
      />
      <UiButton
        type="submit"
        variant="primary"
        size="icon"
        disabled={!value.trim()}
        ariaLabel={aiMode ? spa.answer.submit : spa.search.submit}
        ><ArrowUpIcon size={15} aria-hidden="true" /></UiButton
      >
    </div>
    {#if capabilities.aiEnabled}
      <AiPromptActions>
        <UiToggleGroup
          bind:value={() => (aiMode ? "ai" : "search"), armFromGroup}
          options={modeOptions}
        />
      </AiPromptActions>
    {/if}
  </AiPromptInput>
</form>

<style>
  .composer-form {
    margin-bottom: 1rem;
  }

  .input-row {
    display: flex;
    gap: 0.375rem;
    align-items: flex-end;
    min-width: 0;
  }

  .input-row :global(.ai-prompt-textarea) {
    flex: 1 1 auto;
    min-width: 0;
  }
</style>
