<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  UX-07 `Conversations` section at the top of `/app/history`: the
  owner's threads (title, last activity, turn count). A row resumes the
  thread at `/app/answer?thread=<id>` (App's delegated click routes it
  client-side); delete removes it from the list and the store. Hidden
  while there are no threads.
-->
<script lang="ts">
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import type { createThreadsSection } from "./threads.svelte.js";

  interface ThreadsSectionProps {
    section: ReturnType<typeof createThreadsSection>;
  }

  let { section }: ThreadsSectionProps = $props();
  const h = spa.history;
  const rows = $derived(section.state.rows);
</script>

{#if rows.length > 0}
  <section class="threads" aria-labelledby="threads-title">
    <h2 id="threads-title">{h.threads_title}</h2>
    <ul class="thread-list">
      {#each rows as row (row.id)}
        <li class="thread-row">
          <a class="thread-title" href={row.resumeUrl}>{row.title}</a>
          <span class="thread-meta">{row.turnsLabel} · {row.updated}</span>
          <UiButton size="sm" variant="danger" onclick={() => section.remove(row)}
            >{h.delete}</UiButton
          >
          <span class="row-error" role="status">{row.error}</span>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  /* Styles live in ThreadsSection.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./ThreadsSection.css";
</style>
