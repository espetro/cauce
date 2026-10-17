<!-- MPL-2.0 -->
<script lang="ts">
  interface Props {
    code: string;
    lang?: string;
  }
  let { code, lang = '' }: Props = $props();
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }
</script>

<div class="block">
  <button class="copy" onclick={copy} aria-label="Copy to clipboard">
    {copied ? 'copied' : 'copy'}
  </button>
  <pre><code data-lang={lang}>{code}</code></pre>
</div>

<style>
  .block { position: relative; }
  .copy {
    position: absolute;
    top: 8px;
    right: 8px;
    font: inherit;
    font-size: 11px;
    padding: 3px 9px;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .block:hover .copy,
  .copy:focus-visible { opacity: 1; }
  .copy:hover { color: var(--fg); }
</style>
