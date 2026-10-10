<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/collapsible` — vendored Bits UI `Collapsible`
  (Collapsible.Root/Trigger/Content) restyled on the app tokens; replaces
  every `<details>`/`<summary>` disclosure. One-way data flow: pass
  `bind:open` or `open` + `onOpenChange` (omit all for uncontrolled use —
  `open` is `$bindable` so Root's internal state still reaches the
  trigger snippet below).
  `onOpenChangeComplete` fires after the height animation — the hook for
  lazy payload loads (archive/cache rows). The `trigger` snippet gets
  the current `open` so callers can rotate their own chevron. Height
  animates via `--bits-collapsible-content-height` (compositor-safe,
  never a layout transition). No `name`/`required`/`ariaInvalid` — this
  is a disclosure widget, not a form field.
-->
<script lang="ts">
  import { Collapsible } from "bits-ui";
  import type { Snippet } from "svelte";

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    onOpenChangeComplete?: (open: boolean) => void;
    disabled?: boolean;
    id?: string;
    trigger: Snippet<[open: boolean]>;
    children: Snippet;
  }

  let {
    open = $bindable(false),
    onOpenChange,
    onOpenChangeComplete,
    disabled = false,
    id,
    trigger,
    children,
  }: Props = $props();
</script>

<Collapsible.Root
  class="ui-collapsible"
  bind:open
  {onOpenChange}
  {onOpenChangeComplete}
  {disabled}
>
  <Collapsible.Trigger class="ui-collapsible-trigger" {id} {disabled}>
    {@render trigger(open)}
  </Collapsible.Trigger>
  <Collapsible.Content class="ui-collapsible-content">
    {@render children()}
  </Collapsible.Content>
</Collapsible.Root>

<style>
  :global(.ui-collapsible-trigger) {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  :global(.ui-collapsible-trigger:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-collapsible-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-collapsible-content) {
    overflow: hidden;
  }

  :global(.ui-collapsible-content[data-state="open"]) {
    animation: ui-collapsible-open 160ms ease-out;
  }

  :global(.ui-collapsible-content[data-state="closed"]) {
    animation: ui-collapsible-close 160ms ease-in;
  }

  @keyframes ui-collapsible-open {
    from {
      height: 0;
    }
    to {
      height: var(--bits-collapsible-content-height);
    }
  }

  @keyframes ui-collapsible-close {
    from {
      height: var(--bits-collapsible-content-height);
    }
    to {
      height: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.ui-collapsible-content[data-state="open"]),
    :global(.ui-collapsible-content[data-state="closed"]) {
      animation: none;
    }
  }
</style>
