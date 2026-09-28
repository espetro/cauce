/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { fmt } from "./format.js";
import { parseSseFrame, pumpSse, type SseFetch, type SseResponse } from "./sse.js";
import type { AnswerFrame } from "./types/AnswerFrame.js";
import type { AnswerSource } from "./types/AnswerSource.js";
import type { AnswerTurn } from "./types/AnswerTurn.js";
import type { ApiError } from "./types/ApiError.js";

/** The `done` variant of [`AnswerFrame`]. */
type DoneFrame = Extract<AnswerFrame, { type: "done" }>;

/** The per-turn element refs inside one `.answer-turn` block. */
interface Turn {
  el: HTMLElement;
  status: HTMLElement;
  meta: HTMLElement;
  requestId: HTMLElement;
  steps: HTMLOListElement;
  text: HTMLElement;
  sourcesEl: HTMLElement;
  relatedEl: HTMLElement;
  ungrounded: HTMLElement;
  ungroundedBadge: HTMLElement;
  pathEl: HTMLElement;
  confEl: HTMLElement;
  errorEl: HTMLElement;
  sources: AnswerSource[];
  // W7-03: tool names from the `step` frames, in run order — the
  // retrieval path the path chip renders on `done`.
  toolsRun: string[];
  q: string;
  turn: number;
  terminal: boolean;
}

interface AnswerRefs {
  stream: HTMLElement;
  turnTpl: HTMLTemplateElement | null;
  followupForm: HTMLFormElement | null;
  followupInput: HTMLInputElement | null;
}

interface AnswerDeps {
  fetchImpl?: SseFetch;
}

export interface AnswerSession {
  start: () => Promise<void>;
  startTurn: (q: string) => void;
  beginTurn: (q: string) => Turn;
  handleFrame: (raw: string) => void;
  pump: (res: SseResponse) => Promise<void>;
  history: AnswerTurn[];
}

/** `el.querySelector(sel)` that fails loudly on a markup drift. */
function req<T extends Element>(root: ParentNode, sel: string): T {
  const found = root.querySelector(sel);
  if (!found) throw new Error(`answer turn markup missing ${sel}`);
  return found as T;
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
 * `followupInput`); `S` is the `var SA = {...}` i18n bundle and `Q` the
 * `var Q` query literal the template injects.
 */
export function createAnswerSession(
  refs: AnswerRefs,
  S: AnswerStrings,
  Q: string,
  { fetchImpl = window.fetch?.bind(window) }: AnswerDeps = {},
): AnswerSession {
  const { stream, turnTpl, followupForm, followupInput } = refs;
  const followupBtn = followupForm ? followupForm.querySelector("button") : null;
  // Completed turns, {role: "user"|"assistant", content} — replayed
  // verbatim on the next POST. A failed turn never enters it, so the
  // wire history always ends on an assistant reply.
  const history: AnswerTurn[] = [];
  let turnNo = 0;
  let busy = false;
  let current: Turn | null = null;

  function setBusy(on: boolean): void {
    busy = on;
    stream.setAttribute("aria-busy", on ? "true" : "false");
    if (followupInput) followupInput.disabled = on;
    if (followupBtn) followupBtn.disabled = on;
  }

  function revealFollowup(focus: boolean): void {
    if (!followupForm) return;
    followupForm.hidden = false;
    if (focus && followupInput) followupInput.focus();
  }

  /** The per-turn element refs inside one `.answer-turn` block. */
  function turnRefs(el: HTMLElement): Turn {
    const q = <T extends Element>(sel: string): T => req<T>(el, sel);
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
  function beginTurn(q: string): Turn {
    turnNo += 1;
    let el: HTMLElement;
    if (turnNo === 1) {
      el = req(stream, ".answer-turn");
    } else {
      if (!turnTpl) throw new Error("answer follow-up markup missing #answer-turn-tpl");
      const first = turnTpl.content.firstElementChild;
      if (!first) throw new Error("#answer-turn-tpl is empty");
      el = first.cloneNode(true) as HTMLElement;
      stream.appendChild(el);
    }
    el.dataset.turn = String(turnNo);
    req(el, ".turn-q").textContent = q;
    const T = turnRefs(el);
    T.q = q;
    T.turn = turnNo;
    current = T;
    el.scrollIntoView?.({ block: "nearest" });
    return T;
  }

  function fail(T: Turn, message: string): void {
    T.errorEl.textContent = message;
    T.errorEl.hidden = false;
    T.status.textContent = S.error_status;
    T.terminal = true;
    setBusy(false);
    // A failed turn leaves no history entry — the follow-up box doubles
    // as the retry path (the input keeps its text).
    revealFollowup(false);
  }

  function renderStep(T: Turn, label: string): void {
    const li = document.createElement("li");
    li.textContent = label;
    T.steps.appendChild(li);
    T.status.textContent = label;
  }

  function renderSources(T: Turn, list: AnswerSource[]): void {
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

  // #226: `done.html` is server-rendered + sanitized — inject it, then
  // retarget the `<a class="cite" data-cite="n">` placeholders to this
  // turn's numbered source cards (`#src-<turn>-<n>`). The `.md` class
  // drops pre-wrap so block markup isn't double-spaced.
  function renderAnswer(T: Turn, html: string): void {
    T.text.classList.add("md");
    T.text.innerHTML = html;
    for (const a of T.text.querySelectorAll<HTMLAnchorElement>("a.cite[data-cite]")) {
      const n = a.getAttribute("data-cite");
      if (n) a.setAttribute("href", "#src-" + T.turn + "-" + n);
    }
  }

  function renderDone(T: Turn, done: DoneFrame): void {
    renderAnswer(T, done.html || "");
    if (done.ungrounded) {
      T.ungrounded.hidden = false;
      T.ungroundedBadge.hidden = false;
    }
    // W7-03: the retrieval path as an always-visible chip — which
    // tools ran (search_web/search_archive step frames) plus the
    // cited-source count; a cached replay saw no tool calls this
    // request, so it names only the count; neither means the model
    // answered directly.
    const tools: string[] = [];
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
    T.confEl.dataset.confidence = String(done.confidence);
    T.confEl.textContent = fmt(S.confidence, { n: done.confidence });
    T.confEl.hidden = false;
    const parts: string[] = [];
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
  function dispatch(T: Turn, raw: string): void {
    // The `event:` name is the discriminant; `data` is the serde-tagged
    // `AnswerFrame` variant of the same name (`"type"` inside the JSON
    // mirrors it, so the cast below is the wire contract).
    const { name, data } = parseSseFrame(raw);
    if (!data) return;
    let payload: unknown;
    try {
      payload = JSON.parse(data);
    } catch {
      return fail(T, S.invalid_stream);
    }
    if (name === "step") {
      const frame = payload as Extract<AnswerFrame, { type: "step" }>;
      renderStep(T, frame.label || frame.query || frame.tool);
      if (frame.tool) T.toolsRun.push(frame.tool);
    }
    else if (name === "delta") {
      T.text.appendChild(
        document.createTextNode((payload as Extract<AnswerFrame, { type: "delta" }>).text),
      );
    } else if (name === "sources") {
      T.sources = (payload as Extract<AnswerFrame, { type: "sources" }>).sources;
      renderSources(T, T.sources);
    } else if (name === "done") renderDone(T, payload as DoneFrame);
    else if (name === "error") {
      const frame = payload as Extract<AnswerFrame, { type: "error" }>;
      let message = frame.message || S.stream_failed;
      if (frame.retry_after_s) {
        message += " (" + fmt(S.retry_after, { n: frame.retry_after_s }) + ")";
      }
      fail(T, message);
    }
  }

  /** Stream `res`'s frames into `T`; a non-terminal close fails. */
  async function pump(T: Turn, res: SseResponse): Promise<void> {
    if (!res.body) return fail(T, S.stream_failed);
    await pumpSse(res, (raw) => dispatch(T, raw));
    // The stream closed without a terminal frame: surface that
    // instead of leaving the page "answering..." forever.
    if (!T.terminal) fail(T, S.stream_failed);
  }

  /** POST `data-endpoint` and stream frames; JSON envelope on non-200. */
  async function streamTurn(T: Turn): Promise<void> {
    setBusy(true);
    const body: { q: string; history?: AnswerTurn[] } = { q: T.q };
    if (history.length) body.history = history;
    try {
      const res = await fetchImpl(stream.dataset.endpoint || "", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: stream.dataset.accept || "",
          ...JSON.parse(stream.dataset.headers || "{}"),
        },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        try {
          const env = (await res.json()) as Partial<ApiError> | undefined;
          fail(T, env?.error?.message || S.stream_failed + ": HTTP " + res.status);
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
  function startTurn(q: string): void {
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
    start: () => streamTurn(current as Turn),
    startTurn,
    beginTurn,
    handleFrame: (raw) => dispatch(current as Turn, raw),
    pump: (res) => pump(current as Turn, res),
    history,
  };
}

/**
 * Wire the answer page (no-op elsewhere: `#answer-stream` only renders
 * while `has_query && enabled`, and `var Q`/`var SA` ride the same gate).
 */
export function initAnswerPage(doc: Document = document, deps?: AnswerDeps): void {
  const stream = doc.getElementById("answer-stream");
  if (!stream || window.Q === undefined || !window.SA) return;
  const session = createAnswerSession(
    {
      stream,
      turnTpl: doc.getElementById("answer-turn-tpl") as HTMLTemplateElement | null,
      followupForm: doc.getElementById("answer-followup") as HTMLFormElement | null,
      followupInput: doc.getElementById("followup-q") as HTMLInputElement | null,
    },
    window.SA,
    window.Q,
    deps,
  );
  session.start();
}
