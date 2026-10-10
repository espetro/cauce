<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ai/source-chip` — one source link in a citations rail, vendored
  from sv-prompt-kit's `Source` trigger and restyled on the app
  tokens (the kit's HoverCard preview is deliberately dropped: source
  chips are already links to the full page, a hover preview would be
  chrome for no gain — plan §1.2 targets widgets, not navigation
  links). `id` + `...rest` forward so the assist card can keep its
  `asrc-<n>` citation anchors.
-->
<script lang="ts">
  interface Props {
    id?: string;
    href: string;
    /** Display title; callers pass the host or the raw URL when absent. */
    title?: string;
    /** Host for the favicon pill — omitted renders the title bare. */
    host?: string;
    faviconUrl?: string;
    children?: import("svelte").Snippet;
  }

  let { id, href, title, host, faviconUrl, children }: Props = $props();
</script>

<a class="ai-source-chip" {id} {href} target="_blank" rel="noopener">
  {#if children}
    {@render children()}
  {:else if host}
    {#if faviconUrl}<img src={faviconUrl} width="16" height="16" alt="" loading="lazy" />{/if}
    <span>{host}</span>
  {:else}
    {title || href}
  {/if}
</a>

<style>
  /* Same pill the legacy `.assist-chip` carried. */
  :global(.ai-source-chip) {
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
    transition: border-color 140ms var(--ease-out);
  }

  :global(.ai-source-chip:hover) {
    border-color: var(--accent);
  }

  :global(.ai-source-chip img) {
    border-radius: 50%;
  }

  :global(.ai-source-chip:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
