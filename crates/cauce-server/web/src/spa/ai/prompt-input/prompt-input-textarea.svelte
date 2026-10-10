<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/prompt-input` textarea — the composer field, vendored from
  sv-prompt-kit's PromptInputTextarea: autosizes up to `maxHeight`
  (from the `AiPromptInput` context), Enter submits, Shift+Enter
  inserts a newline, and the context's `disabled` (busy/isLoading)
  locks the field while keeping the submitted text visible. Raw
  `<textarea>` is an allowed primitive per plan §1.2 — the foundation
  part here is the autosize/submit plumbing + the shared composer
  state, not a wrapper around a widget. `ref` is `$bindable` so
  parents can autofocus.
-->
<script lang="ts">
  import { getPromptInputContext } from "./context.svelte.js";

  interface Props {
    ref?: HTMLTextAreaElement | null;
    name?: string;
    placeholder?: string;
    ariaLabel?: string;
    required?: boolean;
    rows?: number;
    disableAutosize?: boolean;
    autofocus?: boolean;
    [key: string]: unknown;
  }

  let {
    ref = $bindable(null),
    name,
    placeholder,
    ariaLabel,
    required = false,
    rows = 1,
    disableAutosize = false,
    autofocus = false,
    ...rest
  }: Props = $props();

  const ctx = getPromptInputContext();

  function syncRef(el: HTMLTextAreaElement | null) {
    ref = el;
    ctx.textareaRef = el;
  }

  // Autosize on content/maxHeight change (replaces the kit's `watch`).
  $effect(() => {
    if (disableAutosize || !ref) return;
    ctx.value; // reactive dep: re-run when the text changes
    ctx.maxHeight;
    ref.style.height = "auto";
    const max = ctx.maxHeight;
    const h = ref.scrollHeight;
    ref.style.height =
      typeof max === "number" ? `${Math.min(h, max)}px` : `min(${h}px, ${max})`;
  });

  function handleKeyDown(e: KeyboardEvent): void {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      ctx.onSubmit?.();
    }
  }

  function handleInput(e: Event): void {
    ctx.setValue((e.currentTarget as HTMLTextAreaElement).value);
  }

  function focus(node: HTMLTextAreaElement): void {
    if (autofocus) node.focus();
  }
</script>

<textarea
  {@attach syncRef}
  {@attach focus}
  class="ai-prompt-textarea"
  {name}
  {placeholder}
  aria-label={ariaLabel}
  {required}
  {rows}
  value={ctx.value}
  disabled={ctx.disabled}
  oninput={handleInput}
  onkeydown={handleKeyDown}
  {...rest}
></textarea>

<style>
  :global(.ai-prompt-textarea) {
    width: 100%;
    min-height: 1.75rem;
    padding: 0.375rem 0.5rem;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 1rem;
    line-height: 1.4;
    resize: none;
    /* The composer box carries the focus border (focus-within on
       .ai-prompt-input) — the field itself takes no ring, matching the
       legacy composer input's `outline: none`. */
    outline: none;
    overflow-y: auto;
  }

  :global(.ai-prompt-textarea::placeholder) {
    color: var(--muted);
  }

  :global(.ai-prompt-textarea:disabled) {
    color: var(--muted);
  }
</style>
