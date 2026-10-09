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
-->
<script lang="ts">
  import { Button } from "bits-ui";
  import type { Snippet } from "svelte";

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
  }: Props = $props();
</script>

{#if href}
  <Button.Root
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

<style>
  :global(.ui-button) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    border: 1px solid transparent;
    border-radius: var(--radius);
    font: inherit;
    text-decoration: none;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    /* Plan §1.3: micro state changes sit in the 120–200ms band on
       compositor properties only — never `transition: all`. */
    transition:
      background-color 140ms var(--ease-out),
      border-color 140ms var(--ease-out),
      color 140ms var(--ease-out),
      opacity 140ms var(--ease-out),
      transform 140ms var(--ease-out);
  }

  :global(.ui-button:active:not(:disabled):not([data-disabled])) {
    transform: scale(0.97);
  }

  /* default: the hairline action — quiet, bordered, never shadowed. */
  :global(.ui-button[data-variant="default"]) {
    border-color: var(--border);
    background: var(--bg);
    color: var(--fg);
  }

  :global(.ui-button[data-variant="default"]:hover:not(:disabled):not([data-disabled])) {
    background: var(--greyed-bg);
  }

  /* primary: accent fill. The hover deepens toward --fg so it reads
     darker on light and lighter on dark without a second token. */
  :global(.ui-button[data-variant="primary"]) {
    background: var(--accent);
    color: var(--bg);
  }

  :global(.ui-button[data-variant="primary"]:hover:not(:disabled):not([data-disabled])) {
    background: color-mix(in srgb, var(--accent) 88%, var(--fg));
  }

  /* danger: warn fill for destructive ops, same recipe as primary. */
  :global(.ui-button[data-variant="danger"]) {
    background: var(--warn);
    color: var(--bg);
  }

  :global(.ui-button[data-variant="danger"]:hover:not(:disabled):not([data-disabled])) {
    background: color-mix(in srgb, var(--warn) 88%, var(--fg));
  }

  /* ghost: chromeless, muted until hovered — inline edit/trigger
     affordances that must not read as primary actions. */
  :global(.ui-button[data-variant="ghost"]) {
    background: transparent;
    color: var(--muted);
  }

  :global(.ui-button[data-variant="ghost"]:hover:not(:disabled):not([data-disabled])) {
    background: var(--greyed-bg);
    color: var(--fg);
  }

  :global(.ui-button[data-size="md"]) {
    min-height: 2rem;
    padding: 0.3rem 0.8rem;
    font-size: 0.9375rem;
  }

  :global(.ui-button[data-size="sm"]) {
    min-height: 1.75rem;
    padding: 0.2rem 0.6rem;
    font-size: 0.8125rem;
  }

  /* icon: square ≥28px hit area (plan §1.3); the icon itself is a
     child element sized by the caller (1rem glyphs). */
  :global(.ui-button[data-size="icon"]) {
    width: 1.75rem;
    height: 1.75rem;
    min-height: 1.75rem;
    padding: 0.25rem;
  }

  /* The single focus-ring recipe from plan §1.3. */
  :global(.ui-button:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  /* `Button.Root` puts `disabled` on <button>; on the <a> form it only
     emits `aria-disabled`/`role`/`tabindex` and leaves onclick live —
     so we pass `data-disabled` ourselves (above) and cover both forms
     here. `pointer-events: none` restores the click suppression the
     anchor path is missing. */
  :global(.ui-button:disabled),
  :global(.ui-button[data-disabled]) {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
</style>
