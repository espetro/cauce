<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  Search Assist (W7-02 port of `web/src/assist.ts`; FX-04 moved it here
  so `features/answer/` owns the AI surfaces — `routes/SearchPage.svelte`
  wires it in): an on-demand card that answers from the already-returned
  result set — the POST carries `context_results`, so no engine re-fetch
  happens. Frame behavior is identical to the HTMX card: `sources`
  renders domain chips up front, `delta` appends raw text, `done`
  injects the server-sanitized `html` and retargets `a.cite[data-cite]`
  to the numbered chips, `error` surfaces the message (+ `retry after
  Ns`). Fire-once per search.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { postAnswer, errorFrom } from "../../lib/api.js";
  import { byokWire } from "../../lib/byok.svelte.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import { faviconUrl, hostOf } from "../../lib/format.js";
  import { AS, spa, fmt } from "../../lib/i18n.js";
  import { parseSseFrame, pumpSse } from "../../lib/sse.js";
  import type { AnswerFrame } from "../../../types/AnswerFrame.js";
  import type { AnswerSource } from "../../../types/AnswerSource.js";

  interface AssistProps {
    /** The page's query (`section[data-q]`). */
    q: string;
    /** Top rows armed for the POST (final order on streamed pages). */
    context: AnswerSource[];
    /** `/app/answer?q=…` link — empty when no answer loop exists. */
    askUrl: string;
    /** Streaming pages render the trigger inert until `meta` arms context. */
    disabled: boolean;
  }

  let { q, context, askUrl, disabled }: AssistProps = $props();

  let fired = $state(false);
  let busy = $state(false);
  let sources = $state<AnswerSource[]>([]);
  let deltas = $state("");
  let html = $state("");
  let confidence = $state<number | null>(null);
  let ungrounded = $state(false);
  let cached = $state(false);
  let errorText = $state("");
  let textEl = $state<HTMLElement>();

  const contextCapped = $derived(context.slice(0, 10));

  function fail(message: string): void {
    errorText = message;
    busy = false;
  }

  // #226: `done.html` is server-rendered + sanitized — inject it, then
  // retarget the `<a class="cite" data-cite="n">` placeholders to the
  // numbered chips (`#asrc-<n>`). The `.md` class drops pre-wrap.
  async function renderAnswer(doneHtml: string): Promise<void> {
    html = doneHtml;
    await tick();
    for (const a of textEl?.querySelectorAll<HTMLAnchorElement>("a.cite[data-cite]") ?? []) {
      const n = a.getAttribute("data-cite");
      if (n) a.setAttribute("href", "#asrc-" + n);
    }
  }

  /** Dispatch one raw SSE frame (text between `\n\n` delimiters). */
  function handleFrame(raw: string): void {
    const { name, data } = parseSseFrame(raw);
    if (!data) return;
    let payload: AnswerFrame;
    try {
      payload = JSON.parse(data);
    } catch {
      return fail(AS.invalid_stream);
    }
    if (name === "sources") {
      const frame = payload as Extract<AnswerFrame, { type: "sources" }>;
      sources = frame.sources;
    } else if (name === "delta") {
      deltas += (payload as Extract<AnswerFrame, { type: "delta" }>).text;
    } else if (name === "done") {
      const done = payload as Extract<AnswerFrame, { type: "done" }>;
      void renderAnswer(done.html || "");
      ungrounded = !!done.ungrounded;
      confidence = done.confidence;
      cached = done.cached;
      busy = false;
    } else if (name === "error") {
      const error = payload as Extract<AnswerFrame, { type: "error" }>;
      let message = error.message || AS.stream_failed;
      if (error.retry_after_s) {
        message += " (" + fmt(AS.retry_after, { n: error.retry_after_s }) + ")";
      }
      fail(message);
    }
  }

  async function fire(): Promise<void> {
    if (fired || !contextCapped.length) return;
    fired = true;
    busy = true;
    try {
      const res = await postAnswer({
        q,
        context_results: contextCapped,
        history: null,
        // PUB-03: BYOK creds, only fields the instance advertises.
        ai: byokWire(capabilities.flags),
      });
      if (!res.ok) {
        const e = await errorFrom(res);
        fail(e.message.startsWith("HTTP ") ? AS.stream_failed + ": " + e.message : e.message);
        return;
      }
      await pumpSse(res, handleFrame);
      // A stream that closes without a terminal frame must not leave
      // the card "answering" forever.
      if (busy) fail(AS.stream_failed);
    } catch {
      fail(AS.stream_failed);
    }
  }
</script>

<!-- The SSR page mounts the section whenever the capability is on;
     the trigger stays inert until `meta` arms `context` (disabled). -->
<section id="assist" class="assist" data-q={q}>
  {#if !fired}
    <button
      type="button"
      id="assist-btn"
      class="assist-trigger"
      aria-controls="assist-card"
      aria-expanded="false"
      {disabled}
      onclick={fire}>{spa.assist.trigger}</button
    >
  {:else}
    <div id="assist-card" class="assist-card" aria-live="polite" aria-busy={busy}>
      <div class="assist-head">
        <span class="assist-label">{spa.assist.label}</span>
        {#if askUrl}
          <a class="assist-ask" href={askUrl}>{spa.assist.ask_ai}</a>
        {/if}
      </div>
      {#if confidence !== null || sources.length || ungrounded || cached}
        <div class="meta">
          {#if sources.length && !ungrounded}
            <span id="assist-grounded" class="meta-chip"
              >{fmt(AS.grounded, { n: sources.length })}</span
            >
          {/if}
          {#if confidence !== null}
            <span
              id="assist-confidence"
              class="meta-chip"
              data-confidence={confidence}>{fmt(AS.confidence, { n: confidence })}</span
            >
          {/if}
          {#if ungrounded}
            <span id="assist-ungrounded" class="meta-chip warn">{AS.ungrounded}</span>
          {/if}
          {#if cached}
            <span id="assist-cached" class="meta-chip">{AS.cached}</span>
          {/if}
        </div>
      {/if}
      <div id="assist-text" class="answer-text" class:md={html.length > 0} bind:this={textEl}>
        {#if html}{@html html}{:else}{deltas}{/if}
      </div>
      {#if sources.length}
        <div id="assist-sources" class="assist-sources">
          {#each sources as src, i (src.url)}
            {@const host = hostOf(src.url)}
            <a
              class="assist-chip"
              id="asrc-{i + 1}"
              href={src.url}
              target="_blank"
              rel="noopener"
            >
              {#if host}
                <img src={faviconUrl(host)} width="16" height="16" alt="" loading="lazy" />
                <span>{host}</span>
              {:else}
                {src.title || src.url}
              {/if}
            </a>
          {/each}
        </div>
      {/if}
      <p class="assist-note">{spa.assist.disclaimer}</p>
      {#if errorText}
        <p id="assist-error" class="field-error" role="alert">{errorText}</p>
      {/if}
  </div>
  {/if}
</section>

<style>
  .assist {
    margin: 0 0 1rem;
  }

  .assist-trigger {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: var(--radius);
    background: var(--accent);
    color: #fff;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .assist-trigger:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .assist-card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--greyed-bg);
    padding: 0.75rem 1rem;
  }

  .assist-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.75rem;
    margin-bottom: 0.25rem;
  }

  .assist-label {
    color: var(--muted);
    font-size: 0.75rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .assist-ask {
    font-size: 0.8125rem;
    white-space: nowrap;
  }

  .assist-sources {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    margin: 0.25rem 0 0.5rem;
  }

  .assist-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
    padding: 0.125rem 0.625rem;
    text-decoration: none;
  }

  .assist-chip:hover {
    border-color: var(--accent);
  }

  .assist-chip img {
    border-radius: 50%;
  }

  .assist-note {
    color: var(--muted);
    font-size: 0.75rem;
    margin: 0.5rem 0 0;
  }
</style>
