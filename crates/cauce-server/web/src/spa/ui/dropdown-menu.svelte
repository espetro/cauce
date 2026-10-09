<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/dropdown-menu` — vendored Bits UI `DropdownMenu`
  (DropdownMenu.Root/Trigger/Portal/Content/Item/Separator) restyled on
  the app tokens. The `trigger` snippet receives the merged trigger
  `props` to spread on the caller's own element
  (`{#snippet trigger(props)} <button {...props}>…` /
  `<UiButton {...props}>`) — delegation avoids nesting a button inside
  the primitive's own. Items are data:

  - `link` entries render a real `<a>` through `Item`'s `child`
    snippet, so operator links keep link semantics (middle-click /
    copy-address work, and App's delegated `a[href]` router still
    intercepts `/app/*` hrefs client-side).
  - `action` entries fire `onSelect` and close the menu.
  - `separator` entries render a hairline.

  `open` + `onOpenChange` are optional one-way props; omit them for
  normal uncontrolled behavior. Not a form field — no `name`/`required`.
-->
<script module lang="ts">
  export interface DropdownLinkItem {
    type: "link";
    label: string;
    href: string;
    current?: boolean;
    disabled?: boolean;
  }

  export interface DropdownActionItem {
    type: "action";
    label: string;
    onSelect: () => void;
    disabled?: boolean;
  }

  export interface DropdownSeparatorItem {
    type: "separator";
  }

  export type DropdownItem =
    | DropdownLinkItem
    | DropdownActionItem
    | DropdownSeparatorItem;
</script>

<script lang="ts">
  import { DropdownMenu } from "bits-ui";
  import type { Snippet } from "svelte";

  interface Props {
    items: DropdownItem[];
    trigger: Snippet<[props: Record<string, unknown>]>;
    ariaLabel?: string;
    align?: "start" | "center" | "end";
    side?: "top" | "right" | "bottom" | "left";
    sideOffset?: number;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    items,
    trigger,
    ariaLabel,
    align = "end",
    side = "bottom",
    sideOffset = 4,
    open,
    onOpenChange,
  }: Props = $props();
</script>

<DropdownMenu.Root {open} {onOpenChange}>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content
      class="ui-dropdown-content"
      {align}
      {side}
      {sideOffset}
      aria-label={ariaLabel}
    >
      {#each items as item, i (i)}
        {#if item.type === "separator"}
          <DropdownMenu.Separator class="ui-dropdown-separator" />
        {:else if item.type === "link"}
          <DropdownMenu.Item
            disabled={item.disabled}
            textValue={item.label}
          >
            {#snippet child({ props })}
              <a
                {...props}
                class="ui-dropdown-item"
                href={item.href}
                aria-current={item.current ? "page" : undefined}
              >
                {item.label}
              </a>
            {/snippet}
          </DropdownMenu.Item>
        {:else}
          <DropdownMenu.Item
            class="ui-dropdown-item"
            disabled={item.disabled}
            textValue={item.label}
            onSelect={() => item.onSelect()}
          >
            {item.label}
          </DropdownMenu.Item>
        {/if}
      {/each}
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>

<style>
  :global(.ui-dropdown-content) {
    min-width: 8rem;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    box-shadow: var(--shadow);
    z-index: 50;
    animation: ui-dropdown-in 160ms var(--ease-out);
  }

  @keyframes ui-dropdown-in {
    from {
      opacity: 0;
      transform: translateY(-2px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  :global(.ui-dropdown-item) {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.4rem;
    border-radius: calc(var(--radius) - 0.2rem);
    color: var(--fg);
    font-size: 0.9375rem;
    text-decoration: none;
    cursor: pointer;
    user-select: none;
    outline: none;
  }

  :global(.ui-dropdown-item[data-highlighted]) {
    background: var(--greyed-bg);
  }

  :global(.ui-dropdown-item[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-dropdown-item[aria-current="page"]) {
    color: var(--accent);
  }

  :global(.ui-dropdown-separator) {
    height: 1px;
    margin: 0.25rem 0.125rem;
    background: var(--border);
  }
</style>
