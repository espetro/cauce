/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { fmt } from "./format.js";

/**
 * W7-01 AI-mode pill (Google AI Mode pattern): a mode switch inside the
 * form, rendered only while the answer loop exists. Arming it swaps the
 * placeholder/submit copy for the ask form's (carried on data- attributes
 * so strings stay in `strings.rs`) and retargets submit at `/answer?q=` —
 * one gesture: type, pick AI, Enter. Without JS it is inert, like the
 * theme toggle.
 */
export function initAiModePill(form, doc = document) {
  const input = form.querySelector('input[name="q"]');
  const aiMode = doc.getElementById("ai-mode");
  const submitBtn = form.querySelector('button[type="submit"]');
  if (!aiMode || !input) return;
  const searchPlaceholder = input.placeholder;
  const searchSubmit = submitBtn ? submitBtn.textContent : "";
  aiMode.addEventListener("click", () => {
    const ai = form.dataset.mode !== "ai";
    form.dataset.mode = ai ? "ai" : "search";
    aiMode.setAttribute("aria-pressed", String(ai));
    input.placeholder = ai ? aiMode.dataset.placeholder : searchPlaceholder;
    if (submitBtn) {
      submitBtn.textContent = ai ? aiMode.dataset.submit : searchSubmit;
    }
    input.focus();
  });
}

/**
 * Submit intercept: a non-empty query navigates to `/search?q=…&stream=1`
 * (or `/answer?q=…` while the AI-mode pill is armed).
 */
export function initSearchForm(form, loc = window.location) {
  const input = form.querySelector('input[name="q"]');
  form.addEventListener("submit", (event) => {
    if (!input || !input.value.trim()) return;
    event.preventDefault();
    if (form.dataset.mode === "ai") {
      loc.assign("/answer?q=" + encodeURIComponent(input.value));
      return;
    }
    const url = new URL(form.action, loc.origin);
    url.searchParams.set("q", input.value);
    url.searchParams.set("stream", "1");
    loc.assign(url.pathname + url.search);
  });
}

/**
 * W5-01: clicking a result queues indexing of that page. Delegated (SSR,
 * streaming and paginated rows alike), keepalive so it survives
 * navigation, and failure-silent — a rejected beacon never breaks the
 * click flow. Armed by `data-index-on-click` on `<body>`; the hash rides
 * `data-query-hash`.
 */
export function initIndexBeacon(doc = document, fetchImpl = window.fetch?.bind(window)) {
  const body = doc.body;
  if (!body || !body.hasAttribute("data-index-on-click") || !fetchImpl) return;
  const queryHash = body.dataset.queryHash || "";
  doc.addEventListener("click", (event) => {
    const a = event.target.closest ? event.target.closest("#results a") : null;
    if (!a || !a.href || !/^https?:/.test(a.href)) return;
    fetchImpl("/api/pages", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Cauce-Client": "ui" },
      body: JSON.stringify(
        queryHash ? { url: a.href, query_hash: queryHash } : { url: a.href },
      ),
      keepalive: true,
    }).catch(() => {});
  });
}

/**
 * The `/search?stream=1` results pump: `cauce:sse` DOM events (dispatched
 * by the htmx `sse` extension) carry `{name, data}`; `data` is JSON.
 * `results` batches append articles (deduped on the normalized key),
 * `meta` renders the final engine-status line + request id + the
 * "new results above" notice, and `error` surfaces the failure.
 *
 * `refs` are the page elements: stream (the `#search-stream` div), results,
 * status, count, metaLine, requestId, above. `S` is the `var S = {...}`
 * i18n bundle the template injects (`stream_strings`).
 */
export function createStreamRenderer(
  refs,
  S,
  { fetchImpl = window.fetch?.bind(window), assist = null } = {},
) {
  const { stream, results, status, count, metaLine, requestId, above } = refs;
  const ERR_KINDS = {
    rate_limited: S.err_rate_limited,
    blocked: S.err_blocked,
    timeout: S.err_timeout,
    parse: S.err_parse,
    transport: S.err_transport,
    no_results: S.err_no_results,
  };
  const queryHash = stream.dataset.queryHash;
  const arrivals = [];
  const seen = new Set();
  // key -> result, for the assist card's context ordering on `meta`.
  const byKey = new Map();

  function appendBatch(batch) {
    batch.results.forEach((result) => {
      const url = result.url;
      // The server dedupes on the normalized URL; dedupe on `key`
      // (its normalized form) so variant spellings cannot render twice.
      const key = result.key || url;
      if (seen.has(key)) return;
      seen.add(key);
      arrivals.push(key);
      byKey.set(key, result);

      const article = document.createElement("article");
      article.dataset.key = key;
      let host = "";
      try {
        host = new URL(url).hostname;
      } catch {
        /* unparsable URL: no icon/host line */
      }
      if (host) {
        const icon = document.createElement("img");
        icon.src = "https://icons.duckduckgo.com/ip3/" + encodeURIComponent(host) + ".ico";
        icon.width = 16;
        icon.height = 16;
        icon.alt = "";
        icon.loading = "lazy";
        article.appendChild(icon);
      }
      const position = arrivals.length - 1;
      const link = document.createElement("a");
      link.href = url;
      link.target = "_blank";
      link.rel = "noopener";
      link.textContent = result.title;
      link.addEventListener("click", () => {
        fetchImpl("/api/click", {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-Cauce-Client": "ui" },
          body: JSON.stringify({
            url: url,
            title: result.title,
            position: position,
            query_hash: queryHash,
          }),
          keepalive: true,
        });
      });
      article.appendChild(link);
      if (host) {
        const hostLine = document.createElement("span");
        hostLine.className = "host";
        hostLine.textContent = host;
        article.appendChild(hostLine);
      }
      const snippet = document.createElement("p");
      snippet.className = "snippet";
      snippet.textContent = result.snippet;
      article.appendChild(snippet);
      results.appendChild(article);
    });
    count.textContent = arrivals.length + " " + S.results;
  }

  function errorKind(value) {
    const kind = typeof value === "string" ? value : Object.keys(value || {})[0];
    return ERR_KINDS[kind] || S.err_unknown;
  }

  function renderMeta(meta) {
    const statuses = meta.engines_used.map((report) => {
      if (report.status === "ok") return report.engine;
      return fmt(S.engine_failed, { engine: report.engine, kind: errorKind(report.status.failed) });
    });
    (meta.engines_skipped || []).forEach((engine) => {
      statuses.push(fmt(S.engine_skipped, { engine: engine }));
    });
    const base =
      meta.source === "network"
        ? fmt(S.live_badge, { ms: meta.elapsed_ms })
        : meta.source.cache && meta.source.cache.stale
          ? S.stale_badge
          : S.cached;
    metaLine.textContent = statuses.length ? base + " · " + statuses.join(" · ") : base;
    requestId.title = meta.request_id;
    requestId.textContent = meta.request_id.slice(0, 8);
    status.textContent = S.complete;
    stream.setAttribute("aria-busy", "false");

    // W7-02: the merged order is final now — arm Assist with the top
    // rows (or hide it when the page answered with nothing).
    if (assist) {
      assist.setContext(meta.order.map((key) => byKey.get(key)).filter((r) => !!r));
    }

    const rank = new Map(meta.order.map((key, index) => [key, index]));
    // Hide anything the final merge did not keep (e.g. a duplicate
    // spelling that streamed before its canonical sibling).
    Array.prototype.forEach.call(results.querySelectorAll("article[data-key]"), (article) => {
      if (!rank.has(article.dataset.key)) article.hidden = true;
    });
    const previousRanks = [];
    let outranking = 0;
    arrivals.forEach((key) => {
      const currentRank = rank.has(key) ? rank.get(key) : Infinity;
      if (previousRanks.some((previousRank) => currentRank < previousRank)) {
        outranking += 1;
      }
      previousRanks.push(currentRank);
    });
    if (outranking > 0) {
      above.textContent = fmt(S.new_above, { n: outranking });
      above.hidden = false;
    }
    if (arrivals.length === 0) {
      const empty = document.createElement("p");
      empty.textContent = statuses.length
        ? S.no_results + " · " + statuses.join(" · ")
        : S.no_results;
      results.appendChild(empty);
    }
  }

  /** Dispatch one `cauce:sse` detail `{name, data}`; `data` is JSON text. */
  function handleMessage(message) {
    let payload;
    try {
      payload = JSON.parse(message.data);
    } catch {
      status.textContent = S.invalid_stream;
      return;
    }
    if (message.name === "results") appendBatch(payload);
    if (message.name === "meta") renderMeta(payload);
    if (message.name === "error") {
      status.textContent = payload.error.message;
      stream.setAttribute("aria-busy", "false");
    }
  }

  return { handleMessage };
}

/**
 * Wire the renderer to `cauce:sse` on the streaming page (no-op on every
 * other page: `#search-stream` only renders while `is_streaming`, and the
 * `var S` i18n bundle rides the same gate).
 */
export function initSearchStream(doc = document, S = window.S, assist = null) {
  const stream = doc.getElementById("search-stream");
  if (!stream || !S) return;
  const renderer = createStreamRenderer(
    {
      stream,
      results: doc.getElementById("results"),
      status: doc.getElementById("stream-status"),
      count: doc.getElementById("result-count"),
      metaLine: doc.getElementById("search-meta"),
      requestId: doc.getElementById("request-id"),
      above: doc.getElementById("new-results-above"),
    },
    S,
    { assist },
  );
  doc.body.addEventListener("cauce:sse", (event) => renderer.handleMessage(event.detail));
}
