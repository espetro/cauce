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
  hero (no air band or marketing sections); both builds show the logotype
  and tagline above the composer.

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
    <!-- The logotype: mark plus wordmark, centred, in both builds. The
         nav hides its own brand on this route so the identity is not
         duplicated. -->
    <a class="logotype" href="/" aria-label={spa.common.brand}>
      <img src={markUrl} alt="" />
      <span>{spa.common.brand}</span>
    </a>
    <!-- DS-20C: the subtitle names the product. One line when it fits;
         otherwise it breaks at the sentence boundary (two spans, each
         unbreakable) instead of mid-phrase. -->
    <p class="subtitle">
      {#each spa.app.home_subtitle.split(/(?<=\.)\s+/) as sentence}
        <span>{sentence}</span>{" "}
      {/each}
    </p>
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
  /* The hero fills the first screen (viewport minus the sticky nav), so
     the scroll cue pinned at the bottom of .air is always above the
     fold, whatever the viewport height. */
  .hero-landing {
    margin: 0;
    padding-top: clamp(1.5rem, 10vh, 8rem);
    min-height: calc(100svh - 5rem);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .hero :global(.subtitle),
  .hero-landing :global(.subtitle) {
    margin-bottom: 1.5rem;
    max-width: 100%;
    color: var(--muted);
    font-size: 1.0625rem;
  }
  .subtitle span {
    display: inline-block;
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
    margin-bottom: 1.25rem;
    color: var(--fg);
    text-decoration: none;
    font-weight: 700;
    font-size: clamp(1.875rem, 5.5vw, 2.75rem);
    letter-spacing: -0.02em;
  }
  /* The mark carries the identity: at owner review round 5 it measured
     68px against Google's 272x92 and DuckDuckGo's 209x54 logos - 3-4x
     too small. Scale to the reference: mark ~104px desktop (the trimmed artwork now fills its box), and the
     wordmark grows with it so the pair stays balanced. The 512px
     source keeps it crisp at 2x. */
  .logotype img {
    width: clamp(4.5rem, 10vw, 6.5rem);
    height: auto;
    display: block;
  }
  /* The air band: deliberate emptiness between the bar and the
     fold-edge content. Tall enough to read as a choice, and what
     pushes the marketing to the first-viewport edge (the scroll cue:
     the section heading just peeks in at the bottom). */
  .air {
    flex: 1;
    min-height: 6rem;
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
    transition:
      color 120ms ease-out,
      border-color 120ms ease-out;
  }
  /* The bob rides the inner icon, not the anchor: a hit target that
     moves under the pointer is hostile to both playwright and people. */
  .scroll-cue :global(svg) {
    animation: cue-bob 2.4s ease-in-out infinite;
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
      transform: translateY(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .scroll-cue :global(svg) {
      animation: none;
    }
  }
  @media (max-width: 640px) {
    .hero-landing {
      padding-top: 6vh;
    }
  }
</style>
