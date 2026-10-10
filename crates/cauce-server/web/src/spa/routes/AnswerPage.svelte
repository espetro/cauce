<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/answer` (FX-04): the multi-turn grounded-answer thread, parity
  with the HTMX `/answer` page. `?q=` starts turn 1; the pinned
  composer appends follow-ups into the same thread. No `q` renders the
  omnibox (AI mode armed — flipping the segment hands off to
  `/app/search`); AI capability off renders the disabled notice. A
  `done.log_id` rewrites the URL to the HTMX `/answer/{id}` so reloads
  read the stored render — that route stays server-rendered by design.
-->
<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { appHref, navigate } from "../app/router.svelte.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { SA } from "../lib/i18n.js";
  import { createAnswerThread } from "../features/answer/thread.svelte.js";
  import AnswerTurn from "../features/answer/AnswerTurn.svelte";
  import FollowupComposer from "../features/answer/FollowupComposer.svelte";
  import Omnibox from "../features/search/Omnibox.svelte";

  interface AnswerPageProps {
    params: URLSearchParams;
  }

  let { params }: AnswerPageProps = $props();

  /** §7.2.3: follow the stream only while the viewport is already at
      the bottom. The scroll-follow hooks below stay hand-rolled on
      purpose: scroll anchoring is behavior, not an interactive widget
      — no bits-ui primitive (and no ui/ wrapper) exists for it, so it
      falls under the migration's justified-exception rule. */
  const FOLLOW_MARGIN = 48;

  function isAtBottom(): boolean {
    const doc = document.documentElement;
    return window.innerHeight + window.scrollY >= doc.scrollHeight - FOLLOW_MARGIN;
  }

  const thread = createAnswerThread({
      isAtBottom,
      scrollToBottom: () => {
        window.scrollTo({ top: document.documentElement.scrollHeight });
      },
      onBeginTurn: () => {
        // The new turn mounts at the tail — a user-initiated scroll to
        // it (same hand-rolled anchor as the follow hooks above).
        void tick().then(() =>
          window.scrollTo({ top: document.documentElement.scrollHeight }),
        );
      },
      onLogId: (id) => {
        history.replaceState({}, "", "/answer/" + id);
      },
  });

  const q = $derived(params.get("q") || "");

  // Bare `/app/answer`: the omnibox is the ask form; its segment toggle
  // decides intent — Search hands off to `/app/search`, AI asks here.
  let askQ = $state("");
  let askAi = $state(true);

  function askSubmit(): void {
    const v = askQ.trim();
    if (!v) return;
    navigate(
      askAi
        ? appHref("/answer?q=" + encodeURIComponent(v))
        : appHref("/search?q=" + encodeURIComponent(v) + "&stream=1"),
    );
  }

  // `?q=` starts turn 1 — gated on the capabilities fetch (it resolves
  // after mount, so the check must be reactive, not onMount).
  let started = $state(false);
  $effect(() => {
    if (!started && q && capabilities.aiEnabled) {
      started = true;
      thread.startTurn(q);
    }
  });

  onDestroy(() => thread.dispose());
</script>

<main class="answer-page">
  {#if !capabilities.loaded}
    <!-- capabilities fetch in flight — render nothing rather than flash -->
  {:else if !capabilities.aiEnabled}
    <p class="answer-disabled">
      {SA.disabled}
      <a href="/settings">{SA.disabled_link}</a>
    </p>
  {:else if !q && !thread.turns.length}
    <h1 class="vh">{SA.submit}</h1>
    <p class="answer-ask">{SA.ask_prompt}</p>
    <Omnibox bind:value={askQ} bind:aiMode={askAi} onsubmit={askSubmit} />
  {:else}
    <!-- Same .vh h1 convention as SearchPage: the running query is the
         page's heading; axe page-has-heading-one. -->
    <h1 class="vh">{q || SA.submit}</h1>
    <div class="answer-thread" aria-live="polite">
      {#each thread.turns as turn (turn.n)}
        <AnswerTurn
          {turn}
          editable={turn.terminal && !turn.error && !turn.stopped && !thread.busy &&
            turn === thread.turns[thread.turns.length - 1]}
          onedit={() => thread.editLast()}
          relatedHref={(r) => appHref("/answer?q=" + encodeURIComponent(r))}
        />
      {/each}
    </div>
    <FollowupComposer
      bind:value={thread.composerText}
      busy={thread.busy}
      onsubmit={(v) => thread.startTurn(v)}
      onstop={() => thread.stop()}
    />
  {/if}
</main>
