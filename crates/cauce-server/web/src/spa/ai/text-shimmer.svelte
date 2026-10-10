<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/text-shimmer` — the streaming status text, vendored from
  sv-prompt-kit's TextShimmer: a moving highlight swept across muted
  text, the live "thinking…" read for step labels while a turn is
  in flight. Restyled on the app tokens (`--muted` -> `--fg` sweep);
  the 4s cycle is a status shimmer, exempt from the §1.3 duration
  budget the same way a spinner is. The global `prefers-reduced-motion`
  guard zeroes the animation (text stays readable: the sweep is a
  background-clip gradient, not the text color).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    as?: "span" | "p" | "div";
    /** Animation period in seconds. */
    duration?: number;
    /** Spread of the highlight, 5–45. */
    spread?: number;
    children: Snippet;
  }

  let { as = "span", duration = 4, spread = 20, children }: Props = $props();

  const s = $derived(Math.min(Math.max(spread, 5), 45));
  const style = $derived(
    `background-image: linear-gradient(to right, var(--muted) ${50 - s}%, var(--fg) 50%, var(--muted) ${50 + s}%); animation-duration: ${duration}s;`,
  );
</script>

<svelte:element this={as} class="ai-text-shimmer" {style}>
  {@render children()}
</svelte:element>

<style>
  :global(.ai-text-shimmer) {
    background-size: 200% auto;
    background-clip: text;
    -webkit-background-clip: text;
    color: transparent;
    animation: ai-shimmer 4s linear infinite;
  }

  @keyframes ai-shimmer {
    from {
      background-position: 100% center;
    }
    to {
      background-position: -100% center;
    }
  }
</style>
