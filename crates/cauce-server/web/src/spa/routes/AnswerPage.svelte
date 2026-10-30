<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/answer` (FX-04): the multi-turn grounded-answer thread, parity
  with the HTMX `/answer` page. `?q=` starts turn 1; the pinned
  composer appends follow-ups into the same thread. No `q` renders the
  omnibox (AI mode armed — flipping the segment hands off to
  `/app/search`); AI capability off renders the disabled notice.
  UX-02: `done.thread_id` rewrites the URL to `/app/answer?thread=<id>`
  — inside the `/app` mount, so reload and back/forward stay in the
  SPA — and `?thread=<id>` hydrates the transcript from
  `GET /api/threads/{id}` (localStorage copy as the fallback). The
  server-rendered `/answer/{log_id}` stays a durable single-answer link.
-->
<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { appHref, navigate } from "../app/router.svelte.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { SA } from "../lib/i18n.js";
  import { createAnswerThread } from "../features/answer/thread.svelte.js";
  import AnswerTurn from "../features/answer/AnswerTurn.svelte";
    import ConversationShell from "../app/ConversationShell.svelte";
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
      onThreadId: (id) => {
        history.replaceState({}, "", appHref("/answer?thread=" + encodeURIComponent(id)));
      },
  });

  const q = $derived(params.get("q") || "");
  const threadParam = $derived(params.get("thread") || "");

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

  // The question that opened the thread — the shell's prompt card.
  const firstQ = $derived(thread.turns[0]?.q || q);

  // `?q=` starts turn 1 — gated on the capabilities fetch (it resolves
  // after mount, so the check must be reactive, not onMount).
  let started = $state(false);
  $effect(() => {
    if (!started && q && capabilities.aiEnabled) {
      started = true;
      thread.startTurn(q);
    }
  });

  // `?thread=<id>` (reload, back/forward, History resume): hydrate it.
  // Re-runs when popstate points the page at another thread; a thread
  // this page just created already carries its own id, so it no-ops.
  $effect(() => {
    const id = threadParam;
    if (id && id !== thread.threadId && capabilities.aiEnabled) {
      thread.reset();
      void thread.hydrate(id);
    }
  });

  onDestroy(() => thread.dispose());
</script>

{#if !capabilities.loaded}
  <!-- capabilities fetch in flight — render nothing rather than flash -->
{:else if !capabilities.aiEnabled}
  <main class="answer-page">
    <p class="answer-disabled">
      {SA.disabled}
      <a href="/settings">{SA.disabled_link}</a>
    </p>
  </main>
{:else}
  <ConversationShell
    heading={firstQ || SA.submit}
    prompt={firstQ}
    docked={thread.turns.length > 0}
  >
    {#snippet composer()}
      {#if thread.turns.length}
        <Omnibox
          followup
          bind:value={thread.composerText}
          busy={thread.busy}
          onstop={() => thread.stop()}
          hint={thread.hint}
          onsubmit={() => thread.startTurn(thread.composerText)}
        />
      {:else if !q && !threadParam}
        <!-- Bare `/app/answer`: the omnibox is the ask form; its segment
             toggle decides intent. -->
        <Omnibox bind:value={askQ} bind:aiMode={askAi} onsubmit={askSubmit} />
      {/if}
    {/snippet}
    {#if !q && !threadParam && !thread.turns.length}
      <p class="answer-ask">{SA.ask_prompt}</p>
    {:else}
      {#if thread.unavailable}
        <p class="answer-disabled">{SA.thread_unavailable}</p>
      {/if}
      <div class="answer-thread" aria-live="polite">
        {#each thread.turns as turn, i (turn.n)}
          <AnswerTurn
            {turn}
            showQuestion={i > 0}
            editable={turn.terminal && !turn.error && !turn.stopped && !thread.busy &&
              turn === thread.turns[thread.turns.length - 1]}
            onedit={() => thread.editLast()}
            relatedHref={(r) => appHref("/answer?q=" + encodeURIComponent(r))}
          />
        {/each}
      </div>
    {/if}
  </ConversationShell>
{/if}
