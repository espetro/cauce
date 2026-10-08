<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  Minimal shell proving the FX-02 pipeline (svelte-check + vite + Tailwind
  tokens). Real routes/components land in FX-03+ — do not grow pages here.
-->
<script lang="ts">
  const version: string = "v3";
</script>

<main class="flex min-h-dvh flex-col items-center justify-center gap-3 bg-bg px-4 text-fg">
  <h1 class="text-2xl font-semibold tracking-tight">cauce <span class="text-muted font-normal">{version}</span></h1>
  <p class="max-w-md text-center text-sm text-muted">
    SPA toolchain online. Routes land in FX-03.
  </p>
</main>
