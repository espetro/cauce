<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The conversation shell (UX-03), shared by `/app/search` and
  `/app/answer`: a top-anchored prompt card (the question that started
  the page), one composer, and a body. Search mode and AI mode fill the
  same three slots — there is no layout fork. The composer is a single
  mounted instance whose position is a pure function of `docked`:
  top-anchored while the thread is empty, sticky at the bottom once it
  has turns (CSS `order`, so focus and typed text survive the move).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface ConversationShellProps {
    /** Visually hidden page heading (axe page-has-heading-one). */
    heading: string;
    /** The question that opened the page; empty hides the card. */
    prompt?: string;
    /** True once the thread has turns: the composer sticks to the bottom. */
    docked?: boolean;
    composer: Snippet;
    children: Snippet;
  }

  let { heading, prompt = "", docked = false, composer, children }: ConversationShellProps =
    $props();
</script>

<main class="conversation-shell">
  <h1 class="vh">{heading}</h1>
  {#if prompt}
    <section class="prompt-card" aria-label={prompt}>
      <p>{prompt}</p>
    </section>
  {/if}
  <div class="shell-composer" data-docked={docked}>
    {@render composer()}
  </div>
  <div class="shell-body">
    {@render children()}
  </div>
</main>

<style>
  /* Styles live in ConversationShell.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./ConversationShell.css";
</style>
