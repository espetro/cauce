<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/tabs` — vendored Bits UI `Tabs` (Tabs.Root/List/Trigger) restyled
  on the app tokens, for the routed tab bars (admin `?tab=`). Panes are
  rendered by the route, so there is deliberately no `Tabs.Content`:
  the caller maps `onValueChange` → `navigate()` and the URL stays the
  source of truth. `activationMode="manual"` because activation here
  means navigation — arrow keys move focus without firing it. One-way
  data flow: `value` + `onValueChange`. The underline is the legacy
  `.tabs` recipe: 2px accent bottom border on the active trigger riding
  the list's 1px hairline (`margin-bottom: -1px` seam). No `name`/
  `required`/`ariaInvalid` — not a form field.
-->
<script lang="ts">
  import { Tabs } from "bits-ui";

  interface TabItem {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value: string;
    tabs: TabItem[];
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    id?: string;
    ariaLabel?: string;
  }

  let {
    value,
    tabs,
    onValueChange,
    disabled = false,
    id,
    ariaLabel,
  }: Props = $props();
</script>

<Tabs.Root
  class="ui-tabs"
  {value}
  {onValueChange}
  activationMode="manual"
  {disabled}
  {id}
>
  <Tabs.List class="ui-tabs-list" aria-label={ariaLabel}>
    {#each tabs as tab (tab.value)}
      <Tabs.Trigger
        class="ui-tabs-trigger"
        value={tab.value}
        disabled={tab.disabled}
      >
        {tab.label}
      </Tabs.Trigger>
    {/each}
  </Tabs.List>
</Tabs.Root>

<style>
  :global(.ui-tabs-list) {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--border);
    margin-bottom: 1rem;
  }

  :global(.ui-tabs-trigger) {
    padding: 0.35rem 0.9rem;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
    transition:
      color 120ms ease-out,
      border-color 120ms ease-out;
  }

  :global(.ui-tabs-trigger[data-state="active"]) {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  :global(.ui-tabs-trigger:hover:not([data-state="active"])) {
    color: var(--fg);
  }

  :global(.ui-tabs-trigger:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-tabs-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.ui-tabs-trigger) {
      transition: none;
    }
  }
</style>
