<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/prompt-input` — the boxed composer root, vendored from
  sv-prompt-kit (Svelte Prompt Kit) and restyled on the app tokens.
  It is the AI-surface composer foundation: `AiPromptTextarea` +
  `AiPromptActions` children share state through the context set here
  (see `./context.svelte.ts`), exactly like the kit's PromptInput —
  value flows one way (`value` + `onValueChange`), Enter submits,
  `isLoading`/`disabled` mark the composer in-flight. `bits-ui`
  tooltip/context plumbing and the kit's `runed` watches are replaced
  by a single `$effect` prop sync; `phosphor-svelte` covers icons.

  Deviations from the vendored source:
  - `compact` is a house variant (data-compact) for the /search row —
    same recipe the legacy `.composer.compact` carried.
  - No TooltipProvider: our actions are labelled buttons/toggles, no
    hover tips (the kit's PromptInputAction with tooltip is not vendored
    — a tooltip need would go through `ui/tooltip` per the plan).
  - The kit's click-to-focus root handler is dropped (the legacy
    `.composer` never had it — the textarea spans the box); the root is
    presentation, not a widget.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    PromptInputState,
    setPromptInputContext,
    type PromptInputSchema,
  } from "./context.svelte.js";

  interface Props extends PromptInputSchema {
    compact?: boolean;
    children: Snippet;
  }

  let {
    compact = false,
    isLoading = false,
    value = "",
    onValueChange,
    maxHeight = 240,
    onSubmit,
    disabled = false,
    children,
  }: Props = $props();

  const ctx = new PromptInputState();
  setPromptInputContext(ctx);

  // One-way prop -> context sync (replaces the kit's per-prop `watch`).
  $effect(() => {
    ctx.isLoading = isLoading;
    ctx.disabled = disabled || isLoading;
    ctx.value = value;
    ctx.onValueChange = onValueChange;
    ctx.maxHeight = maxHeight;
    ctx.onSubmit = onSubmit;
  });
</script>

<div
  class="ai-prompt-input"
  data-compact={compact || undefined}
  data-disabled={ctx.disabled || undefined}
>
  {@render children()}
</div>

<style>
  /* The boxed composer — same recipe the legacy `.composer` carried:
     hairline, 1.5-tier radius, accent border on focus-within. */
  :global(.ai-prompt-input) {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding: 0.5rem;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    background: var(--bg);
    cursor: text;
    transition: border-color 140ms var(--ease-out);
  }

  :global(.ai-prompt-input:focus-within) {
    border-color: var(--accent);
  }

  :global(.ai-prompt-input[data-compact]) {
    padding: 0.375rem;
  }
</style>
