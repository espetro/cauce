/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Shared state for the `ai/prompt-input` composer (vendored from
 * sv-prompt-kit — the Svelte Prompt Kit registry — and adapted to the
 * house conventions): `AiPromptInput` sets this context so the
 * `AiPromptTextarea` / `AiPromptActions` children find the composer's
 * value, disabled flag and submit hook without prop drilling.
 */

import { getContext, setContext } from "svelte";

/** The composer's public contract — mirrors the kit's PromptInputSchema. */
export interface PromptInputSchema {
  isLoading?: boolean;
  value?: string;
  onValueChange?: (value: string) => void;
  maxHeight?: number | string;
  onSubmit?: () => void;
  disabled?: boolean;
}

export class PromptInputState {
  isLoading = $state(false);
  value = $state("");
  maxHeight = $state<number | string>(240);
  onSubmit = $state<(() => void) | undefined>(undefined);
  disabled = $state(false);
  textareaRef = $state<HTMLTextAreaElement | null>(null);
  onValueChange = $state<((value: string) => void) | undefined>(undefined);

  setValue(newValue: string): void {
    this.value = newValue;
    this.onValueChange?.(newValue);
  }
}

const PROMPT_INPUT_KEY = Symbol("prompt-input");

export function setPromptInputContext(ctx: PromptInputState): PromptInputState {
  return setContext(PROMPT_INPUT_KEY, ctx);
}

export function getPromptInputContext(): PromptInputState {
  const ctx = getContext<PromptInputState>(PROMPT_INPUT_KEY);
  if (!ctx) {
    throw new Error("AiPromptInput.* components must render inside <AiPromptInput>");
  }
  return ctx;
}
