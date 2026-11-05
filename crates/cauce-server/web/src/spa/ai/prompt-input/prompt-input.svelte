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
  /* Styles live in prompt-input.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./prompt-input.css";
</style>
