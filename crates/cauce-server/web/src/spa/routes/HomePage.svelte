<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app` landing: the full-size composer. Submit routes to
  `/app/search?q=…&stream=1` (or hands off to `/answer?q=` in AI mode).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { navigate } from "../app/router.svelte.js";
  import { createSearchPage } from "../features/search/search.svelte.js";
  import { spa } from "../lib/i18n.js";
  import { LANDING } from "../lib/flags.js";
  import Omnibox from "../features/search/Omnibox.svelte";
  import Landing from "../features/landing/Landing.svelte";

  // The submit hand-off lives in the feature state (`submit` decides
  // search vs AI-mode navigation); the landing page only drives it.
  const page = createSearchPage();

  onMount(() => {
    document.title = spa.common.brand;
  });
</script>

<main>
  <h1 class="vh">{spa.common.brand}</h1>
  <div class:hero={!LANDING} class:hero-landing={LANDING}>
    <Omnibox
      bind:value={page.q}
      bind:aiMode={page.aiMode}
      autofocus
      onsubmit={() => page.submit(navigate)}
    />
  </div>
  {#if LANDING}
    <Landing />
  {/if}
</main>

<style>
  .hero {
    margin: 18vh 0 0;
  }
  .hero-landing {
    margin: 10vh 0 0;
  }
</style>
