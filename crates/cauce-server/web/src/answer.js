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
 * The `/answer?q=` thread session (W7-04): each exchange is one
 * `.answer-turn` (the user line `.turn-q`, the streamed reply, that
 * turn's sources/related). Turn 1 is the SSR shell under
 * `#answer-stream`; follow-ups clone `#answer-turn-tpl` and POST
 * `/api/answer` with the completed prior turns as `history` — threads
 * are ephemeral page state (a reload starts fresh; a server-side
 * `threads` table is a documented follow-up).
 *
 * Per turn the session renders `step` frames as a progress line,
 * `delta` text live, `sources` as numbered cards the `[n]` citation
 * markers link to (card ids are `src-<turn>-<n>` so citations never
 * point at another turn's list), and the terminal `done`/`error` frame
 * as metadata or an inline error. A settled turn reveals
 * `#answer-followup`, the bottom-pinned next-question form.
 *
 * `refs` are the page elements (`stream`, `turnTpl`, `followupForm`,
 * `followupInput`); `S` is the `var S = {...}` i18n bundle and `Q` the
 * `var Q` query literal the template injects.
 */
export function createAnswerSession(refs, S, Q, { fetchImpl = window.fetch?.bind(window) } = {}) {
  const { stream, turnTpl, followupForm, followupInput } = refs;
  const followupBtn = followupForm ? followupForm.querySelector("button") : null;
  // Completed turns, {role: "user"|"assistant", content} — replayed
  // verbatim on the next POST. A failed turn never enters it, so the
  // wire history always ends on an assistant reply.
  const history = [];
  let turnNo = 0;
  let busy = false;
  let current = null;

  function setBusy(on) {
    busy = on;
    stream.setAttribute("aria-busy", on ? "true" : "false");
    if (followupInput) followupInput.disabled = on;
    if (followupBtn) followupBtn.disabled = on;
  }

  function revealFollowup(focus) {
    if (!followupForm) return;
    followupForm.hidden = false;
    if (focus && followupInput) followupInput.focus();
  }

  /** The per-turn element refs inside one `.answer-turn` block. */
  function turnRefs(el) {
    const q = (sel) => el.querySelector(sel);
    return {
      el,
      status: q(".answer-status"),
      meta: q(".answer-meta"),
      requestId: q(".request-id"),
      steps: q(".answer-steps"),
      text: q(".answer-text"),
      sourcesEl: q(".answer-sources"),
      relatedEl: q(".answer-related"),
      ungrounded: q(".ungrounded"),
      ungroundedBadge: q(".answer-ungrounded-badge"),
      pathEl: q(".answer-path"),
      confEl: q(".answer-confidence"),
      errorEl: q(".answer-error"),
      sources: [],
      // W7-03: tool names from the `step` frames, in run order — the
      // retrieval path the path chip renders on `done`.
      toolsRun: [],
      q: "",
      turn: 0,
      terminal: false,
    };
  }

  /**
   * Attach the next turn to the thread: the SSR shell for turn 1, a
   * `#answer-turn-tpl` clone for every later one.
   */
  function beginTurn(q) {
    turnNo += 1;
    let el;
    if (turnNo === 1) {
      el = stream.querySelector(".answer-turn");
    } else {
      el = turnTpl.content.firstElementChild.cloneNode(true);
      stream.appendChild(el);
    }
    el.dataset.turn = String(turnNo);
    el.querySelector(".turn-q").textContent = q;
    const T = turnRefs(el);
    T.q = q;
    T.turn = turnNo;
    current = T;
    if (el.scrollIntoView) el.scrollIntoView({ block: "nearest" });
    return T;
  }

  function fail(T, message) {
    T.errorEl.textContent = message;
    T.errorEl.hidden = false;
    T.status.textContent = S.error_status;
    T.terminal = true;
    setBusy(false);
    // A failed turn leaves no history entry — the follow-up box doubles
    // as the retry path (the input keeps its text).
    revealFollowup(false);
  }

  function renderStep(T, label) {
    const li = document.createElement("li");
    li.textContent = label;
    T.steps.appendChild(li);
    T.status.textContent = label;
  }

  function renderSources(T, list) {
    if (!list.length) return;
    const heading = document.createElement("p");
    heading.className = "section-label";
    heading.textContent = S.sources;
    T.sourcesEl.appendChild(heading);
    list.forEach((src, i) => {
      const card = document.createElement("article");
      card.className = "source-card";
      card.id = "src-" + T.turn + "-" + (i + 1);
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
      T.sourcesEl.appendChild(card);
    });
  }

  // Re-render the accumulated answer with [n] markers as anchor
  // links into this turn's numbered source cards below.
  function renderAnswer(T, body) {
    T.text.textContent = "";
    const re = /\[(\d+)\]/g;
    let last = 0;
    let m;
    while ((m = re.exec(body)) !== null) {
      T.text.appendChild(document.createTextNode(body.slice(last, m.index)));
      const n = parseInt(m[1], 10);
      const anchor = "src-" + T.turn + "-" + n;
      if (n >= 1 && n <= T.sources.length && T.el.ownerDocument.getElementById(anchor)) {
        const a = document.createElement("a");
        a.className = "cite";
        a.href = "#" + anchor;
        a.textContent = m[0];
        T.text.appendChild(a);
      } else {
        T.text.appendChild(document.createTextNode(m[0]));
      }
      last = re.lastIndex;
    }
    T.text.appendChild(document.createTextNode(body.slice(last)));
  }

  function renderDone(T, done) {
    renderAnswer(T, done.answer || "");
    if (done.ungrounded) {
      T.ungrounded.hidden = false;
      T.ungroundedBadge.hidden = false;
    }
    // W7-03: the retrieval path as an always-visible chip — which
    // tools ran (search_web/search_archive step frames) plus the
    // cited-source count; a cached replay saw no tool calls this
    // request, so it names only the count; neither means the model
    // answered directly.
    const tools = [];
    for (const t of T.toolsRun) {
      const word = t === "search_web" ? S.tool_web : t === "search_archive" ? S.tool_archive : t;
      if (!tools.includes(word)) tools.push(word);
    }
    if (tools.length) {
      T.pathEl.dataset.path = "searched";
      T.pathEl.textContent = fmt(S.path_searched, { tools: tools.join(" + "), n: T.sources.length });
    } else if (T.sources.length) {
      T.pathEl.dataset.path = "searched";
      T.pathEl.textContent = fmt(S.path_replay, { n: T.sources.length });
    } else {
      T.pathEl.dataset.path = "direct";
      T.pathEl.textContent = S.path_direct;
    }
    T.pathEl.hidden = false;
    T.confEl.dataset.confidence = done.confidence;
    T.confEl.textContent = fmt(S.confidence, { n: done.confidence });
    T.confEl.hidden = false;
    const parts = [];
    if (done.model) parts.push(done.model);
    if (done.cached) parts.push(S.cached);
    T.meta.textContent = parts.join(" · ");
    if (done.request_id) {
      T.requestId.title = done.request_id;
      T.requestId.textContent = done.request_id.slice(0, 8);
      T.requestId.hidden = false;
    }
    (done.related_questions || []).forEach((rq, i) => {
      if (i === 0) {
        const heading = document.createElement("p");
        heading.className = "section-label";
        heading.textContent = S.related;
        T.relatedEl.appendChild(heading);
      }
      const p = document.createElement("p");
      p.className = "related-question";
      const a = document.createElement("a");
      a.href = "/answer?q=" + encodeURIComponent(rq);
      a.textContent = rq;
      p.appendChild(a);
      T.relatedEl.appendChild(p);
    });
    T.status.textContent = S.complete;
    T.terminal = true;
    // The turn is complete: it joins the replayed thread, the input is
    // free for the next question.
    history.push({ role: "user", content: T.q });
    history.push({ role: "assistant", content: done.answer || "" });
    if (followupInput) followupInput.value = "";
    setBusy(false);
    revealFollowup(true);
  }

  /** Dispatch one raw SSE frame (text between `\n\n` delimiters). */
  function dispatch(T, raw) {
    const { name, data } = parseSseFrame(raw);
    if (!data) return;
    let payload;
    try {
      payload = JSON.parse(data);
    } catch {
      return fail(T, S.invalid_stream);
    }
    if (name === "step") {
      renderStep(T, payload.label || payload.query || payload.tool);
      if (payload.tool) T.toolsRun.push(payload.tool);
    }
    else if (name === "delta") T.text.appendChild(document.createTextNode(payload.text || ""));
    else if (name === "sources") {
      T.sources = payload.sources || [];
      renderSources(T, T.sources);
    } else if (name === "done") renderDone(T, payload);
    else if (name === "error") {
      let message = payload.message || S.stream_failed;
      if (payload.retry_after_s) {
        message += " (" + fmt(S.retry_after, { n: payload.retry_after_s }) + ")";
      }
      fail(T, message);
    }
  }

  /** Read `res.body` to end, dispatching each `\n\n`-delimited frame. */
  async function pump(T, res) {
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const chunk = await reader.read();
      buffer += decoder.decode(chunk.value, { stream: !chunk.done });
      let i;
      while ((i = buffer.indexOf("\n\n")) >= 0) {
        dispatch(T, buffer.slice(0, i));
        buffer = buffer.slice(i + 2);
      }
      if (chunk.done) break;
    }
    if (buffer.trim()) dispatch(T, buffer);
    // The stream closed without a terminal frame: surface that
    // instead of leaving the page "answering..." forever.
    if (!T.terminal) fail(T, S.stream_failed);
  }

  /** POST `data-endpoint` and stream frames; JSON envelope on non-200. */
  async function streamTurn(T) {
    setBusy(true);
    const body = { q: T.q };
    if (history.length) body.history = history;
    try {
      const res = await fetchImpl(stream.dataset.endpoint, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: stream.dataset.accept,
          ...JSON.parse(stream.dataset.headers || "{}"),
        },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        try {
          const env = await res.json();
          fail(T, (env && env.error && env.error.message) || S.stream_failed + ": HTTP " + res.status);
        } catch {
          fail(T, S.stream_failed + ": HTTP " + res.status);
        }
        return;
      }
      await pump(T, res);
    } catch {
      fail(T, S.stream_failed);
    }
  }

  /** A follow-up question: new turn node, then stream it. */
  function startTurn(q) {
    if (busy || !q || !q.trim()) return;
    streamTurn(beginTurn(q));
  }

  if (followupForm && followupInput) {
    followupForm.addEventListener("submit", (e) => {
      e.preventDefault();
      startTurn(followupInput.value.trim());
    });
  }

  // Turn 1 is the SSR shell for `?q=`; begin it eagerly so frames fed
  // without `start()` (tests) still have somewhere to land.
  beginTurn(Q);

  return {
    start: () => streamTurn(current),
    startTurn,
    beginTurn,
    handleFrame: (raw) => dispatch(current, raw),
    pump: (res) => pump(current, res),
    history,
  };
}

/**
 * Wire the answer page (no-op elsewhere: `#answer-stream` only renders
 * while `has_query && enabled`, and `var Q`/`var S` ride the same gate).
 */
export function initAnswerPage(doc = document, deps) {
  const stream = doc.getElementById("answer-stream");
  if (!stream || window.Q === undefined || !window.S) return;
  const followupForm = doc.getElementById("answer-followup");
  const session = createAnswerSession(
    {
      stream,
      turnTpl: doc.getElementById("answer-turn-tpl"),
      followupForm,
      followupInput: doc.getElementById("followup-q"),
    },
    window.S,
    window.Q,
    deps,
  );
  session.start();
}
