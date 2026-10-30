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
  UX-03: this is the ONE composer of the conversation shell. `followup`
  reshapes it for a running thread — no mode segment, the follow-up
  placeholder, the `busy` state swapping send for stop (interruption is
  a feature; the text survives so the user can edit + resubmit), and the
  history-cap `hint`. Where it sits (top / sticky bottom) is the
  shell's call, never this component's.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import { ArrowUpIcon, StopIcon } from "phosphor-svelte";
  import { SA, spa } from "../../lib/i18n.js";
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
    /** Thread-follow-up shape: no mode segment, stop while busy. */
    followup?: boolean;
    busy?: boolean;
    onstop?: () => void;
    /** History-cap warning (empty = none). */
    hint?: string;
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
    followup = false,
    busy = false,
    onstop,
    hint = "",
    onsubmit,
  }: OmniboxProps = $props();

  const placeholder = $derived.by(() => {
    if (followup) return SA.followup_placeholder;
    return aiMode ? spa.answer.placeholder : spa.search.placeholder;
  });

  const submitLabel = $derived.by(() => {
    if (followup) return SA.followup_submit;
    return aiMode ? spa.answer.submit : spa.search.submit;
  });

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
  id={followup ? "answer-followup" : undefined}
  onsubmit={(e) => {
    e.preventDefault();
    onsubmit();
  }}
>
  <AiPromptInput {value} onValueChange={(v) => (value = v)} isLoading={busy} onSubmit={onsubmit} {compact}>
    <div class="input-row">
      <AiPromptTextarea
        bind:ref={inputEl}
        {autofocus}
        id={followup ? "followup-q" : undefined}
        name="q"
        autocomplete="off"
        {placeholder}
        ariaLabel={placeholder}
      />
      {#if busy && onstop}
        <UiButton
          {...({ id: "answer-stop" } as ComponentProps<typeof UiButton>)}
          size="icon"
          onclick={onstop}
          ariaLabel={SA.stop}><StopIcon size={15} aria-hidden="true" /></UiButton
        >
      {:else}
        <UiButton
          type="submit"
          variant="primary"
          size="icon"
          disabled={!value.trim()}
          ariaLabel={submitLabel}
          ><ArrowUpIcon size={15} aria-hidden="true" /></UiButton
        >
      {/if}
    </div>
    {#if capabilities.aiEnabled && !followup}
      <AiPromptActions>
        <UiToggleGroup
          bind:value={() => (aiMode ? "ai" : "search"), armFromGroup}
          options={modeOptions}
        />
      </AiPromptActions>
    {/if}
  </AiPromptInput>
  {#if hint}
    <p class="composer-hint" role="status">{hint}</p>
  {/if}
</form>

<style>
  .composer-hint {
    margin: 0.25rem 0 0;
    font-size: 0.8125rem;
    color: var(--muted);
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
