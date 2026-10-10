<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/prompt-input` actions — the composer's tool row, vendored from
  sv-prompt-kit's PromptInputActions: a flex row for the actions the
  composer carries (mode segment, send/stop). Interactive children are
  composed by the caller from `ui/` wrappers — the foundation supplies
  layout only. `end` right-aligns (the send/stop cluster convention).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    end?: boolean;
    children: Snippet;
  }

  let { end = false, children }: Props = $props();
</script>

<div class="ai-prompt-actions" data-align={end ? "end" : undefined}>
  {@render children()}
</div>

<style>
  :global(.ai-prompt-actions) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.125rem;
  }

  :global(.ai-prompt-actions[data-align="end"]) {
    justify-content: flex-end;
  }
</style>
