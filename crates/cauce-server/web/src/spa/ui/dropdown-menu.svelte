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
  // The component's Tailwind composition (dropdown-menu.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./dropdown-menu.css";

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
              <!-- A disabled link keeps no `href`: bits-ui's disabled
                Item short-circuits its own handler WITHOUT calling
                preventDefault, so an anchor that still carries href
                would navigate anyway. Dropping the attribute is the
                correct semantics too — a disabled link is not a link. -->
              <a
                {...props}
                class="ui-dropdown-item"
                href={item.disabled ? undefined : item.href}
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

