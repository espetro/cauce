<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/loader` — the streaming-state indicator, vendored from
  sv-prompt-kit's `typing` loader variant: three dots pulsing in
  sequence, the "thinking" read for an in-flight answer. Only this
  variant is vendored (the kit ships twelve — a quiet status line
  needs exactly one). Decorative: `aria-hidden` — the status label
  next to it carries the text. The global `prefers-reduced-motion`
  guard in app.css already zeroes the animation.
-->
<script lang="ts">
  interface Props {
    /** Dot diameter in px (sm=4, md=6, lg=8). */
    size?: "sm" | "md" | "lg";
  }

  let { size = "md" }: Props = $props();
</script>

<span class="ai-loader" data-size={size} aria-hidden="true">
  <span class="ai-loader-dot"></span>
  <span class="ai-loader-dot"></span>
  <span class="ai-loader-dot"></span>
</span>

<style>
  :global(.ai-loader) {
    display: inline-flex;
    align-items: center;
    gap: 0.1875rem;
  }

  :global(.ai-loader-dot) {
    border-radius: 50%;
    background: var(--muted);
    animation: ai-loader-pulse 1s ease-in-out infinite;
  }

  :global(.ai-loader[data-size="sm"] .ai-loader-dot) {
    width: 0.25rem;
    height: 0.25rem;
  }

  :global(.ai-loader[data-size="md"] .ai-loader-dot) {
    width: 0.375rem;
    height: 0.375rem;
  }

  :global(.ai-loader[data-size="lg"] .ai-loader-dot) {
    width: 0.5rem;
    height: 0.5rem;
  }

  :global(.ai-loader-dot:nth-child(2)) {
    animation-delay: 250ms;
  }

  :global(.ai-loader-dot:nth-child(3)) {
    animation-delay: 500ms;
  }

  @keyframes ai-loader-pulse {
    0%,
    100% {
      opacity: 0.3;
      transform: translateY(0);
    }
    50% {
      opacity: 1;
      transform: translateY(-0.125rem);
    }
  }
</style>
