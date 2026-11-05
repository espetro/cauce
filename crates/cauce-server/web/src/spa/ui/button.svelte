<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/button` — vendored Bits UI `Button` restyled on the app tokens
  (plan §7.3 "Bits UI + owned ui/ wrappers"; bound part: `Button.Root`).
  This is the design-system button: a raw `<button>` in a feature file
  is a defect per plan §1.2.

  Deliberate deviations from the raw part:
  - `variant` + `size` are the house API: default (hairline), primary
    (accent fill), danger (warn fill, destructive ops), ghost
    (chromeless, quiet until hover); md / sm / icon (≥28px square hit
    area per plan §1.3).
  - `type` defaults to "button" so a UiButton inside a form never
    submits by accident — pass `type="submit"` deliberately.
  - `href` renders an `<a>`: `Button.Root` keeps link semantics, and
    the two branches below exist only because the part's prop union
    forbids `type` on the anchor form.
  - `...rest` forwards undeclared attributes — delegated trigger props
    (`id`, `tabindex`, `data-state`, `aria-describedby`, pointer/focus
    handlers) from `UiTooltip`/`UiDropdownMenu` `{#snippet trigger(props)}`
    call sites reach the underlying element through it. The house props
    (`variant`/`size`/`class`/…) are applied after the spread so callers
    cannot override the styling contract.
-->
<script lang="ts">
  import { Button } from "bits-ui";
  import type { Snippet } from "svelte";
  // The component's Tailwind composition (button.css) is a plain
  // stylesheet imported from the script: the build resolves its
  // `@apply` against the app theme, and svelte-check's CSS parser —
  // which does not know Tailwind's at-rules — never sees them.
  import "./button.css";

  interface Props {
    variant?: "default" | "primary" | "danger" | "ghost";
    size?: "md" | "sm" | "icon";
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
    href?: string;
    ariaLabel?: string;
    children?: Snippet;
  }

  let {
    variant = "default",
    size = "md",
    type = "button",
    disabled = false,
    onclick,
    href,
    ariaLabel,
    children,
    ...rest
  }: Props = $props();
</script>

{#if href}
  <Button.Root
    {...rest}
    class="ui-button"
    data-variant={variant}
    data-size={size}
    data-disabled={disabled || undefined}
    {href}
    {disabled}
    {onclick}
    aria-label={ariaLabel}
  >
    {@render children?.()}
  </Button.Root>
{:else}
  <Button.Root
    {...rest}
    class="ui-button"
    data-variant={variant}
    data-size={size}
    data-disabled={disabled || undefined}
    {type}
    {disabled}
    {onclick}
    aria-label={ariaLabel}
  >
    {@render children?.()}
  </Button.Root>
{/if}

