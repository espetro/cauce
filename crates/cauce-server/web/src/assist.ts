/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*
 * W7-02 Search Assist (the DDG "Search Assist" / Kagi "Quick Answer"
 * shape): an on-demand card above the results that answers from the
 * already-returned set — the POST carries those rows as
 * `context_results`, so no engine re-fetch happens. On `?stream=1`
 * pages the trigger renders disabled and the stream's `meta` frame
 * calls `setContext` with the merged order (wired in app.ts via the
 * search renderer's `onMeta` hook).
 *
 * `AS` is the `var AS = {...}` i18n bundle and `assistContext` the
 * serialized top-K rows; both ride an `{% if assist %}` bootstrap
 * script, so the feature is inert without them (same convention as
 * `var S`/`var Q`).
 */

import { fmt } from "./format.js";
import { parseSseFrame, pumpSse } from "./sse.js";
import type { SseFetch } from "./sse.js";
import type { AnswerFrame } from "./types/AnswerFrame.js";
import type { AnswerSource } from "./types/AnswerSource.js";
import type { ApiError } from "./types/ApiError.js";

/** What `initAssist` hands back to the search stream's `meta` hook. */
export interface AssistHandle {
  setContext(rows: AnswerSource[]): void;
}

export function initAssist(
  doc: Document = document,
  AS: AssistStrings | undefined = window.AS,
  initialContext: AnswerSource[] | undefined = window.assistContext,
  fetchImpl: SseFetch | undefined = window.fetch?.bind(window),
): AssistHandle | null {
  const sectionEl = doc.getElementById("assist");
  const btnEl = doc.getElementById("assist-btn");
  if (!sectionEl || !(btnEl instanceof HTMLButtonElement) || !AS || !fetchImpl) return null;
  // The `{% if assist %}` template block renders the card's elements all
  // or none — `section`/`btn` being present gates the rest.
  const section = sectionEl;
  const btn = btnEl;
  const card = doc.getElementById("assist-card")!;
  const text = doc.getElementById("assist-text")!;
  const srcEl = doc.getElementById("assist-sources")!;
  const err = doc.getElementById("assist-error")!;
  // W7-03 grounded/confidence chips in the card meta row.
  const meta = doc.getElementById("assist-meta")!;
  const grounded = doc.getElementById("assist-grounded")!;
  const conf = doc.getElementById("assist-confidence")!;
  const ung = doc.getElementById("assist-ungrounded")!;
  const cachedChip = doc.getElementById("assist-cached")!;

  let context = (initialContext || []).slice(0, 10);
  let sources: AnswerSource[] = [];
  let fired = false;

  function setContext(list: AnswerSource[]): void {
    context = list.slice(0, 10);
    if (context.length) {
      btn.disabled = false;
    } else {
      section.hidden = true;
    }
  }

  function fail(message: string): void {
    err.textContent = message;
    err.hidden = false;
    card.setAttribute("aria-busy", "false");
  }

  // Always-visible chips (favicon + domain), one per grounded source —
  // they render on the up-front `sources` frame, before any answer text
  // lands.
  function renderSources(list: AnswerSource[]): void {
    sources = list;
    srcEl.textContent = "";
    list.forEach((src, i) => {
      const chip = doc.createElement("a");
      chip.className = "assist-chip";
      chip.id = "asrc-" + (i + 1);
      chip.href = src.url;
      chip.target = "_blank";
      chip.rel = "noopener";
      let host = "";
      try {
        host = new URL(src.url).hostname;
      } catch {
        /* unparsable URL: fall back to title text */
      }
      if (host) {
        const icon = doc.createElement("img");
        icon.src = "https://icons.duckduckgo.com/ip3/" + encodeURIComponent(host) + ".ico";
        icon.width = 16;
        icon.height = 16;
        icon.alt = "";
        icon.loading = "lazy";
        chip.appendChild(icon);
        const name = doc.createElement("span");
        name.textContent = host;
        chip.appendChild(name);
      } else {
        chip.textContent = src.title || src.url;
      }
      srcEl.appendChild(chip);
    });
    // W7-03: the grounded chip renders the moment the up-front
    // `sources` frame lands — before any answer text.
    if (list.length) {
      grounded.textContent = fmt(AS.grounded, { n: list.length });
      grounded.hidden = false;
      meta.hidden = false;
    }
  }

  // Re-render the accumulated answer with [n] markers as anchor links
  // into the numbered chips (the /answer page's convention).
  function renderAnswer(body: string): void {
    text.textContent = "";
    const re = /\[(\d+)\]/g;
    let last = 0;
    let m;
    while ((m = re.exec(body)) !== null) {
      text.appendChild(doc.createTextNode(body.slice(last, m.index)));
      const n = parseInt(m[1], 10);
      if (n >= 1 && n <= sources.length && doc.getElementById("asrc-" + n)) {
        const a = doc.createElement("a");
        a.className = "cite";
        a.href = "#asrc-" + n;
        a.textContent = m[0];
        text.appendChild(a);
      } else {
        text.appendChild(doc.createTextNode(m[0]));
      }
      last = re.lastIndex;
    }
    text.appendChild(doc.createTextNode(body.slice(last)));
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
      renderSources((payload as Extract<AnswerFrame, { type: "sources" }>).sources);
    } else if (name === "delta") {
      text.appendChild(
        doc.createTextNode((payload as Extract<AnswerFrame, { type: "delta" }>).text),
      );
    } else if (name === "done") {
      const done = payload as Extract<AnswerFrame, { type: "done" }>;
      renderAnswer(done.answer);
      // W7-03: confidence + grounded state from `done` — assist is
      // grounded on the shown results by construction, so an
      // `ungrounded` badge here means an empty context slipped in.
      if (done.ungrounded) {
        grounded.hidden = true;
        ung.hidden = false;
      }
      conf.textContent = fmt(AS.confidence, { n: done.confidence });
      conf.dataset.confidence = String(done.confidence);
      conf.hidden = false;
      if (done.cached) cachedChip.hidden = false;
      meta.hidden = false;
      card.setAttribute("aria-busy", "false");
    } else if (name === "error") {
      const error = payload as Extract<AnswerFrame, { type: "error" }>;
      let message = error.message || AS.stream_failed;
      if (error.retry_after_s) {
        message += " (" + fmt(AS.retry_after, { n: error.retry_after_s }) + ")";
      }
      fail(message);
    }
  }

  btn.addEventListener("click", () => {
    if (fired || !context.length) return;
    fired = true;
    btn.hidden = true;
    btn.setAttribute("aria-expanded", "true");
    card.hidden = false;
    card.setAttribute("aria-busy", "true");
    fetchImpl("/api/answer", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "text/event-stream",
        "X-Cauce-Client": "ui",
      },
      body: JSON.stringify({ q: section.dataset.q, context_results: context }),
    })
      .then((res) => {
        if (!res.ok) {
          return res.json().then(
            (env) => {
              fail(
                (env as Partial<ApiError> | undefined)?.error?.message ||
                  AS.stream_failed + ": HTTP " + res.status,
              );
            },
            () => {
              fail(AS.stream_failed + ": HTTP " + res.status);
            },
          );
        }
        return pumpSse(res, handleFrame).then(() => {
          // A stream that closes without a terminal frame must not
          // leave the card "answering" forever.
          if (card.getAttribute("aria-busy") === "true") fail(AS.stream_failed);
        });
      })
      .catch(() => {
        fail(AS.stream_failed);
      });
  });

  return { setContext };
}
