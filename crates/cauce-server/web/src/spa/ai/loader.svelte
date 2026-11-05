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
  // Global styles (loader.css) are a plain stylesheet imported from
  // the script: the build resolves `@apply` and these classes stay
  // global as they are used across component boundaries.
  import "./loader.css";

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


