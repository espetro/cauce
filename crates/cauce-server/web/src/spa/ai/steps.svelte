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

<style>
  /* Same quiet read as the legacy steps disclosure: muted, small,
     never a live progress banner. */
  :global(.ai-steps) {
    margin: 0 0 0.5rem;
    color: var(--muted);
    font-size: 0.875rem;
  }

  :global(.ai-steps-summary) {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  :global(.ai-steps-chevron) {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }

  :global(.ai-steps-chevron.open) {
    transform: rotate(90deg);
  }

  /* The kit's vertical bar + indented items (replaces the legacy
     inline dot-join — one step per line reads better at length). */
  :global(.ai-steps-body) {
    display: grid;
    grid-template-columns: min-content minmax(0, 1fr);
    gap: 0.75rem;
    margin-top: 0.375rem;
  }

  :global(.ai-steps-bar) {
    width: 0.125rem;
    height: 100%;
    background: var(--border);
    border-radius: 1px;
  }

  :global(.ai-steps-body ol) {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  :global(.ai-steps-body li) {
    padding: 0.0625rem 0;
  }
</style>
