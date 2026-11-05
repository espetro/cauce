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
      <!-- DS-20C: the subtitle names the product (the eyebrow chip was
          dropped per owner review). The vh h1 stays the axe heading. -->
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
  .hero :global(.subtitle) {
    margin-bottom: 1.25rem;
    max-width: 36rem;
  }
  /* The composer: a column flex container shrink-wraps children, so
     the form must carry its own measure — this is what keeps the bar
     full-size on both builds (the 254px regression). */
  .hero :global(.composer-form),
  .hero-landing :global(.composer-form) {
    width: min(42rem, 100%);
  }
  /* The logotype stacks: mark on top, wordmark below (owner review). */
  .logotype {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 1.75rem;
    color: var(--fg);
    text-decoration: none;
    font-weight: 700;
    font-size: clamp(1.6rem, 5vw, 2.4rem);
    letter-spacing: -0.02em;
  }
  .logotype img {
    width: clamp(2.75rem, 7vw, 3.5rem);
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
