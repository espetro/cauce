<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/steps` — the collapsed step rail on an answer turn, vendored
  from sv-prompt-kit's `Steps` and composed on `UiCollapsible`
  (bits-ui stays the disclosure primitive — the foundation layers the
  AI-specific presentation: chevron trigger, vertical bar, per-line
  items — on top of the ui/ wrapper, never re-implements it).
  Closed by default like the legacy `.answer-steps` disclosure.
-->
<script lang="ts">
  import { CaretRightIcon } from "phosphor-svelte";
  import UiCollapsible from "../ui/collapsible.svelte";
  // Global styles (steps.css) are a plain stylesheet imported from
  // the script: the build resolves `@apply` and these classes stay
  // global as they are used across component boundaries.
  import "./steps.css";

  interface Props {
    /** The trigger label (e.g. the `steps · n` i18n string). */
    label: string;
    /** Step lines in arrival order. */
    steps: string[];
  }

  let { label, steps }: Props = $props();
</script>

<div class="ai-steps">
  <UiCollapsible>
    {#snippet trigger(open)}
      <span class="ai-steps-summary">
        <span class="ai-steps-chevron" class:open aria-hidden="true"
          ><CaretRightIcon size={12} style="vertical-align: -0.125em" /></span
        >{label}
      </span>
    {/snippet}
    <div class="ai-steps-body">
      <span class="ai-steps-bar" aria-hidden="true"></span>
      <ol>
        {#each steps as step}
          <li>{step}</li>
        {/each}
      </ol>
    </div>
  </UiCollapsible>
</div>


