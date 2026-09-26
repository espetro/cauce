/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { fmt } from "./format.js";

/**
 * Split one raw SSE frame (`event:`/`data:` lines) into `{name, data}`.
 * Multiple `data:` lines concatenate (matching the historical inline
 * behavior); frames without data return `data: ""` and are skipped by
 * the caller.
 */
export function parseSseFrame(raw) {
  let name = "message";
  let data = "";
  raw.split("\n").forEach((line) => {
    if (line.indexOf("event:") === 0) name = line.slice(6).trim();
    else if (line.indexOf("data:") === 0) data += line.slice(5).trim();
  });
  return { name, data };
}

/**
 * The `/answer?q=` stream session: POSTs `data-endpoint` (the SSE-over-fetch
 * exchange `EventSource` cannot do) and renders `step` frames as a progress
 * line, `delta` text live, `sources` as numbered cards the `[n]` citation
 * markers link to, and the terminal `done`/`error` frame as metadata or an
 * inline error.
 *
 * `refs` are the page elements; `S` is the `var S = {...}` i18n bundle and
 * `Q` the `var Q` query literal the template injects.
 */
export function createAnswerSession(refs, S, Q, { fetchImpl = window.fetch?.bind(window) } = {}) {
  const {
    stream,
    status,
    meta,
    requestId,
    steps,
    text,
    sourcesEl,
    relatedEl,
    ungrounded,
    ungroundedBadge,
    pathEl,
    confEl,
    errorEl,
  } = refs;
  let sources = [];
  // W7-03: tool names from the `step` frames, in run order — the
  // retrieval path the path chip renders on `done`.
  let toolsRun = [];

  function fail(message) {
    errorEl.textContent = message;
    errorEl.hidden = false;
    status.textContent = S.error_status;
    stream.setAttribute("aria-busy", "false");
  }

  function renderStep(label) {
    const li = document.createElement("li");
    li.textContent = label;
    steps.appendChild(li);
    status.textContent = label;
  }

  function renderSources(list) {
    if (!list.length) return;
    const heading = document.createElement("p");
    heading.className = "section-label";
    heading.textContent = S.sources;
    sourcesEl.appendChild(heading);
    list.forEach((src, i) => {
      const card = document.createElement("article");
      card.className = "source-card";
      card.id = "src-" + (i + 1);
      let host = "";
      try {
        host = new URL(src.url).hostname;
      } catch {
        /* unparsable URL: no icon/host line */
      }
      const badge = document.createElement("span");
      badge.className = "cite-badge";
      badge.textContent = "[" + (i + 1) + "]";
      card.appendChild(badge);
      if (host) {
        const icon = document.createElement("img");
        icon.src = "https://icons.duckduckgo.com/ip3/" + encodeURIComponent(host) + ".ico";
        icon.width = 16;
        icon.height = 16;
        icon.alt = "";
        icon.loading = "lazy";
        card.appendChild(icon);
      }
      const link = document.createElement("a");
      link.href = src.url;
      link.target = "_blank";
      link.rel = "noopener";
      link.textContent = src.title || src.url;
      card.appendChild(link);
      if (host) {
        const hostLine = document.createElement("span");
        hostLine.className = "host";
        hostLine.textContent = host;
        card.appendChild(hostLine);
      }
      if (src.snippet) {
        const snippet = document.createElement("p");
        snippet.className = "snippet";
        snippet.textContent = src.snippet;
        card.appendChild(snippet);
      }
      sourcesEl.appendChild(card);
    });
  }

  // Re-render the accumulated answer with [n] markers as anchor
  // links into the numbered source cards below.
  function renderAnswer(body) {
    text.textContent = "";
    const re = /\[(\d+)\]/g;
    let last = 0;
    let m;
    while ((m = re.exec(body)) !== null) {
      text.appendChild(document.createTextNode(body.slice(last, m.index)));
      const n = parseInt(m[1], 10);
      if (n >= 1 && n <= sources.length && document.getElementById("src-" + n)) {
        const a = document.createElement("a");
        a.className = "cite";
        a.href = "#src-" + n;
        a.textContent = m[0];
        text.appendChild(a);
      } else {
        text.appendChild(document.createTextNode(m[0]));
      }
      last = re.lastIndex;
    }
    text.appendChild(document.createTextNode(body.slice(last)));
  }

  function renderDone(done) {
    renderAnswer(done.answer || "");
    if (done.ungrounded) {
      ungrounded.hidden = false;
      ungroundedBadge.hidden = false;
    }
    // W7-03: the retrieval path as an always-visible chip — which
    // tools ran (search_web/search_archive step frames) plus the
    // cited-source count; a cached replay saw no tool calls this
    // request, so it names only the count; neither means the model
    // answered directly.
    const tools = [];
    for (const t of toolsRun) {
      const word = t === "search_web" ? S.tool_web : t === "search_archive" ? S.tool_archive : t;
      if (!tools.includes(word)) tools.push(word);
    }
    if (tools.length) {
      pathEl.dataset.path = "searched";
      pathEl.textContent = fmt(S.path_searched, { tools: tools.join(" + "), n: sources.length });
    } else if (sources.length) {
      pathEl.dataset.path = "searched";
      pathEl.textContent = fmt(S.path_replay, { n: sources.length });
    } else {
      pathEl.dataset.path = "direct";
      pathEl.textContent = S.path_direct;
    }
    pathEl.hidden = false;
    confEl.dataset.confidence = done.confidence;
    confEl.textContent = fmt(S.confidence, { n: done.confidence });
    confEl.hidden = false;
    const parts = [];
    if (done.model) parts.push(done.model);
    if (done.cached) parts.push(S.cached);
    meta.textContent = parts.join(" · ");
    if (done.request_id) {
      requestId.title = done.request_id;
      requestId.textContent = done.request_id.slice(0, 8);
    }
    (done.related_questions || []).forEach((rq, i) => {
      if (i === 0) {
        const heading = document.createElement("p");
        heading.className = "section-label";
        heading.textContent = S.related;
        relatedEl.appendChild(heading);
      }
      const p = document.createElement("p");
      p.className = "related-question";
      const a = document.createElement("a");
      a.href = "/answer?q=" + encodeURIComponent(rq);
      a.textContent = rq;
      p.appendChild(a);
      relatedEl.appendChild(p);
    });
    status.textContent = S.complete;
    stream.setAttribute("aria-busy", "false");
  }

  /** Dispatch one raw SSE frame (text between `\n\n` delimiters). */
  function handleFrame(raw) {
    const { name, data } = parseSseFrame(raw);
    if (!data) return;
    let payload;
    try {
      payload = JSON.parse(data);
    } catch {
      return fail(S.invalid_stream);
    }
    if (name === "step") {
      renderStep(payload.label || payload.query || payload.tool);
      if (payload.tool) toolsRun.push(payload.tool);
    }
    else if (name === "delta") text.appendChild(document.createTextNode(payload.text || ""));
    else if (name === "sources") {
      sources = payload.sources || [];
      renderSources(sources);
    } else if (name === "done") renderDone(payload);
    else if (name === "error") {
      let message = payload.message || S.stream_failed;
      if (payload.retry_after_s) {
        message += " (" + fmt(S.retry_after, { n: payload.retry_after_s }) + ")";
      }
      fail(message);
    }
  }

  /** Read `res.body` to end, dispatching each `\n\n`-delimited frame. */
  async function pump(res) {
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const chunk = await reader.read();
      buffer += decoder.decode(chunk.value, { stream: !chunk.done });
      let i;
      while ((i = buffer.indexOf("\n\n")) >= 0) {
        handleFrame(buffer.slice(0, i));
        buffer = buffer.slice(i + 2);
      }
      if (chunk.done) break;
    }
    if (buffer.trim()) handleFrame(buffer);
    // The stream closed without a terminal frame: surface that
    // instead of leaving the page "answering..." forever.
    if (stream.getAttribute("aria-busy") === "true") fail(S.stream_failed);
  }

  /** POST `data-endpoint` and stream frames; JSON envelope on non-200. */
  async function start() {
    try {
      const res = await fetchImpl(stream.dataset.endpoint, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: stream.dataset.accept,
          ...JSON.parse(stream.dataset.headers || "{}"),
        },
        body: JSON.stringify({ q: Q }),
      });
      if (!res.ok) {
        try {
          const env = await res.json();
          fail((env && env.error && env.error.message) || S.stream_failed + ": HTTP " + res.status);
        } catch {
          fail(S.stream_failed + ": HTTP " + res.status);
        }
        return;
      }
      await pump(res);
    } catch {
      fail(S.stream_failed);
    }
  }

  return { handleFrame, pump, start };
}

/**
 * Wire the answer page (no-op elsewhere: `#answer-stream` only renders
 * while `has_query && enabled`, and `var Q`/`var S` ride the same gate).
 */
export function initAnswerPage(doc = document, deps) {
  const stream = doc.getElementById("answer-stream");
  if (!stream || window.Q === undefined || !window.S) return;
  const session = createAnswerSession(
    {
      stream,
      status: doc.getElementById("answer-status"),
      meta: doc.getElementById("answer-meta"),
      requestId: doc.getElementById("request-id"),
      steps: doc.getElementById("answer-steps"),
      text: doc.getElementById("answer-text"),
      sourcesEl: doc.getElementById("answer-sources"),
      relatedEl: doc.getElementById("answer-related"),
      ungrounded: doc.getElementById("answer-ungrounded"),
      ungroundedBadge: doc.getElementById("answer-ungrounded-badge"),
      pathEl: doc.getElementById("answer-path"),
      confEl: doc.getElementById("answer-confidence"),
      errorEl: doc.getElementById("answer-error"),
    },
    window.S,
    window.Q,
    deps,
  );
  session.start();
}
