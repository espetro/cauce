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
  /* Styles live in prompt-input-actions.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./prompt-input-actions.css";
</style>
