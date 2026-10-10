<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The pinned follow-up composer (§7.1/§7.2), on the `ai/prompt-input`
  foundation (DS-AI): `AiPromptInput`'s `isLoading` maps the §7.2.2
  busy state — the textarea locks while holding the submitted
  question and the action swaps `ask` for `stop` (interruption is a
  feature; the text survives so the user can edit + resubmit). Send
  and stop are icon UiButtons — the foundation ships no buttons; the
  square-stop glyph is the ai-elements convention. `stop` keeps the
  default variant: an interruption affordance, not a destructive op.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import { ArrowUpIcon, StopIcon } from "phosphor-svelte";
  import AiPromptInput from "../../ai/prompt-input/prompt-input.svelte";
  import AiPromptTextarea from "../../ai/prompt-input/prompt-input-textarea.svelte";
  import AiPromptActions from "../../ai/prompt-input/prompt-input-actions.svelte";
  import UiButton from "../../ui/button.svelte";
  import { SA } from "../../lib/i18n.js";

  interface ComposerProps {
    value: string;
    busy: boolean;
    onsubmit: (q: string) => void;
    onstop: () => void;
  }

  let { value = $bindable(""), busy, onsubmit, onstop }: ComposerProps = $props();

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    onsubmit(value);
  }
</script>

<form id="answer-followup" onsubmit={submit}>
  <AiPromptInput {value} onValueChange={(v) => (value = v)} isLoading={busy} onSubmit={() => onsubmit(value)}>
    <div class="input-row">
      <AiPromptTextarea
        id="followup-q"
        name="q"
        required
        placeholder={SA.followup_placeholder}
        ariaLabel={SA.followup_placeholder}
      />
      {#if busy}
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
          ariaLabel={SA.followup_submit}
          ><ArrowUpIcon size={15} aria-hidden="true" /></UiButton
        >
      {/if}
    </div>
  </AiPromptInput>
</form>

<style>
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
