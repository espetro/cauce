<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app` landing (W9-05): the DuckDuckGo/Google shape — logotype centred
  directly above the search bar, a reserved air band below it, and the
  marketing content starting at the first-viewport edge so the scroll
  cue is a hint of the next section, not a wall of text. The wordmark
  appears here only; other surfaces keep the small mark in the nav (the
  TopNav hides its own brand on this route so the two don't double).

  The non-landing build (self-hosted `cauce serve`) keeps the plain app
  hero: eyebrow + subtitle above the full-size composer.

  Submit routes to `/app/search?q=…&stream=1` (or hands off to
  `/answer?q=` in AI mode).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { navigate } from "../app/router.svelte.js";
  import { createSearchPage } from "../features/search/search.svelte.js";
  import { spa } from "../lib/i18n.js";
  import { LANDING } from "../lib/flags.js";
  import markUrl from "../assets/cauce-mark.png";
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
    <!-- The logotype: mark plus wordmark, centred, in both builds —
         the issue's ask is the landing home's identity, not a deploy
         flag. The nav hides its own brand on this route so the
         identity is not duplicated. -->
    <a class="logotype" href="/" aria-label={spa.common.brand}>
      <img src={markUrl} alt="" />
      <span>{spa.common.brand}</span>
    </a>
    {#if !LANDING}
      <!-- DS-20C: the hero names the product. The vh h1 stays the axe
          heading; the eyebrow + subtitle answer "what is this?" for a
          first-time visitor without scrolling. -->
      <p class="eyebrow">{spa.app.home_eyebrow}</p>
      <p class="subtitle">{spa.app.home_subtitle}</p>
    {/if}
    <Omnibox
      bind:value={page.q}
      bind:aiMode={page.aiMode}
      autofocus
      onsubmit={() => page.submit(navigate)}
    />
    {#if LANDING}
      <!-- The reserved air band: intentionally empty (a trending or
           plugin slot lands here later). Tall enough to read as a
           choice, and what pushes the marketing to the fold edge. -->
      <div class="air" aria-hidden="true"></div>
    {/if}
  </div>
  {#if LANDING}
    <Landing />
  {/if}
</main>

<style>
  .hero {
    margin: 18vh 0 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .hero-landing {
    margin: 16vh 0 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .hero :global(.eyebrow) {
    margin-bottom: 0.625rem;
  }
  .hero :global(.subtitle) {
    margin-bottom: 1.25rem;
    max-width: 36rem;
  }
  .logotype {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 1.5rem;
    color: var(--fg);
    text-decoration: none;
    font-weight: 700;
    font-size: clamp(1.6rem, 5vw, 2.4rem);
    letter-spacing: -0.02em;
  }
  .logotype img {
    width: clamp(2rem, 6vw, 2.75rem);
    height: auto;
    display: block;
  }
  /* The air band: deliberate emptiness between the bar and the
     fold-edge content. Tall enough to read as a choice, and what
     pushes the marketing to the first-viewport edge (the scroll cue:
     the section heading just peeks in at the bottom). */
  .air {
    height: clamp(10rem, 42vh, 26rem);
  }
  @media (max-width: 640px) {
    .hero-landing {
      margin-top: 10vh;
    }
  }
</style>
