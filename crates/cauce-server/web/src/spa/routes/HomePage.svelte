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
  import { ArrowDownIcon } from "phosphor-svelte";
  import { navigate } from "../app/router.svelte.js";
  import { createSearchPage } from "../features/search/search.svelte.js";
  import { spa } from "../lib/i18n.js";
  import { LANDING } from "../lib/flags.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
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
           plugin slot lands here later). The scroll cue sits at its
           bottom edge — a slow circular bounce pointing at the first
           marketing section just below the fold. -->
      <div class="air">
        <a
          class="scroll-cue"
          href="#one-server"
          aria-label={spa.landing.scroll_cue}
          onclick={(e) => {
            e.preventDefault();
            document.querySelector("#one-server")?.scrollIntoView({ behavior: "smooth" });
          }}
        >
          <ArrowDownIcon size={16} weight="bold" aria-hidden="true" />
        </a>
      </div>
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
  /* The mark carries the identity next to a 38px wordmark: it must read
     as the logo, not a bullet. 3.25rem-4.25rem keeps the two optically
     balanced (the 2048px source downscaled to 512 keeps it crisp at
     2x). */
  .logotype img {
    width: clamp(3.25rem, 8.5vw, 4.25rem);
    height: auto;
    display: block;
  }
  /* The air band: deliberate emptiness between the bar and the
     fold-edge content. Tall enough to read as a choice, and what
     pushes the marketing to the first-viewport edge (the scroll cue:
     the section heading just peeks in at the bottom). */
  .air {
    height: clamp(10rem, 42vh, 26rem);
    display: flex;
    align-items: flex-end;
    justify-content: center;
  }
  /* The scroll cue: a circular button that bobs, pointing at the
     first marketing section below the fold. */
  .scroll-cue {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--muted);
    text-decoration: none;
    margin-bottom: clamp(1rem, 4vh, 2.5rem);
    animation: cue-bob 2.4s ease-in-out infinite;
    transition:
      color 120ms ease-out,
      border-color 120ms ease-out;
  }
  .scroll-cue:hover {
    color: var(--fg);
    border-color: var(--fg);
  }
  .scroll-cue:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @keyframes cue-bob {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(6px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .scroll-cue {
      animation: none;
    }
  }
  @media (max-width: 640px) {
    .hero-landing {
      margin-top: 10vh;
    }
  }
</style>
