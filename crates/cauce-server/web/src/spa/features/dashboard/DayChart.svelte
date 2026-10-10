<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The searches-per-day stacked-bar SVG — the `day_bars` geometry that
  used to be precomputed in `src/dashboard.rs`, now client-side.
  `<title>` on each rect stays (plan §3.12): it is already the
  accessible hover on SVG, and wrapping 60 rect bars in UiTooltip
  triggers is friction with no payoff.
-->
<script lang="ts">
  import { CHART_H_TICKS, CHART_W, type DayBar } from "./dashboard.svelte.js";

  interface DayChartProps {
    bars: DayBar[];
    ariaLabelledby: string;
  }

  let { bars, ariaLabelledby }: DayChartProps = $props();
</script>

<svg
  class="bars"
  viewBox="0 0 {CHART_W} {CHART_H_TICKS}"
  role="img"
  aria-labelledby={ariaLabelledby}
>
  {#each bars as bar}
    <rect
      class="network"
      x={bar.x}
      y={bar.netY}
      width={bar.w}
      height={bar.netH}><title>{bar.title}</title></rect
    >
    <rect
      class="cache"
      x={bar.x}
      y={bar.cacheY}
      width={bar.w}
      height={bar.cacheH}><title>{bar.title}</title></rect
    >
    <text x={bar.x} y={CHART_H_TICKS}>{bar.tick}</text>
  {/each}
</svg>

<style>
  svg.bars {
    width: 100%;
    height: auto;
    display: block;
  }
  svg.bars rect.cache {
    fill: var(--accent);
  }
  svg.bars rect.network {
    fill: var(--muted);
    opacity: 0.45;
  }
  svg.bars text {
    fill: var(--muted);
    font-size: 8px;
  }
</style>
