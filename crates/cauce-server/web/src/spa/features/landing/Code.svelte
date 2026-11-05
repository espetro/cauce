<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/.

     A syntax highlighted code block for the landing sections. highlight.js
     core plus three grammars (bash, yaml, json) keeps the cost small; the
     snippets are static strings and hljs escapes its input, so {@html}
     only ever sees highlighted, escaped markup. Long lines wrap instead
     of scrolling, so a snippet is always fully visible. -->
<script lang="ts">
  import hljs from "highlight.js/lib/core";
  import json from "highlight.js/lib/languages/json";
  import yaml from "highlight.js/lib/languages/yaml";

  // The stock bash grammar only colors shell builtins, so `curl`, `mise`
  // and `cargo` stay plain. This is a minimal install-snippet grammar:
  // comments, quoted strings, URLs, the command word, subcommands, flags.
  hljs.registerLanguage("shell", () => ({
    contains: [
      hljs.HASH_COMMENT_MODE,
      hljs.APOS_STRING_MODE,
      { scope: "string", match: /https?:\/\/[^\s'"]+/ },
      { scope: "built_in", match: /^(?:curl|mise|cargo|cauce)\b/ },
      { scope: "built_in", match: /(?<=\| )sh\b/ },
      { scope: "keyword", match: /\b(?:install|use|serve)\b/ },
      { scope: "meta", match: /(?<=\s)--?[A-Za-z][\w-]*/ },
    ],
  }));
  hljs.registerLanguage("json", json);
  hljs.registerLanguage("yaml", yaml);

  let { code, lang }: { code: string; lang: "shell" | "json" | "yaml" } = $props();

  const html = $derived(hljs.highlight(code, { language: lang }).value);
</script>

<pre class="code"><code>{@html html}</code></pre>

<style>
  .code {
    margin: 0;
    padding: 12px 14px;
    background: var(--greyed-bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.55;
    min-width: 0;
    max-width: 100%;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  /* Token colors: AA contrast on --greyed-bg in both themes. */
  .code :global(.hljs-comment) {
    color: var(--muted);
    font-style: italic;
  }
  .code :global(.hljs-string),
  .code :global(.hljs-regexp) {
    color: light-dark(#0b6b4b, #7ee0a8);
  }
  .code :global(.hljs-built_in),
  .code :global(.hljs-keyword),
  .code :global(.hljs-literal) {
    color: light-dark(#6d28d9, #c4b5fd);
  }
  .code :global(.hljs-attr),
  .code :global(.hljs-attribute),
  .code :global(.hljs-property) {
    color: var(--accent);
  }
  .code :global(.hljs-number),
  .code :global(.hljs-bullet) {
    color: light-dark(#9a4a06, #fbbf24);
  }
  .code :global(.hljs-meta),
  .code :global(.hljs-variable),
  .code :global(.hljs-symbol) {
    color: light-dark(#0e6a82, #67e8f9);
  }
</style>
