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
 * calls `setContext` with the merged order (wired in app.js via the
 * search renderer's `onMeta` hook).
 *
 * `AS` is the `var AS = {...}` i18n bundle and `assistContext` the
 * serialized top-K rows; both ride an `{% if assist %}` bootstrap
 * script, so the feature is inert without them (same convention as
 * `var S`/`var Q`).
 */
export function initAssist(
  doc = document,
  AS = window.AS,
  initialContext = window.assistContext,
  fetchImpl = window.fetch?.bind(window),
) {
  const section = doc.getElementById("assist");
  const btn = doc.getElementById("assist-btn");
  const card = doc.getElementById("assist-card");
  const text = doc.getElementById("assist-text");
  const srcEl = doc.getElementById("assist-sources");
  const err = doc.getElementById("assist-error");
  if (!section || !btn || !AS || !fetchImpl) return null;

  let context = (initialContext || []).slice(0, 10);
  let sources = [];
  let fired = false;

  function setContext(list) {
    context = list.slice(0, 10);
    if (context.length) {
      btn.disabled = false;
    } else if (section) {
      section.hidden = true;
    }
  }

  function fail(message) {
    err.textContent = message;
    err.hidden = false;
    card.setAttribute("aria-busy", "false");
  }

  // Always-visible chips (favicon + domain), one per grounded source —
  // they render on the up-front `sources` frame, before any answer text
  // lands.
  function renderSources(list) {
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
  }

  // Re-render the accumulated answer with [n] markers as anchor links
  // into the numbered chips (the /answer page's convention).
  function renderAnswer(body) {
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

  function handleFrame(raw) {
    let name = "message";
    let data = "";
    raw.split("\n").forEach((line) => {
      if (line.indexOf("event:") === 0) name = line.slice(6).trim();
      else if (line.indexOf("data:") === 0) data += line.slice(5).trim();
    });
    if (!data) return;
    let payload;
    try {
      payload = JSON.parse(data);
    } catch {
      return fail(AS.invalid_stream);
    }
    if (name === "sources") renderSources(payload.sources || []);
    else if (name === "delta") text.appendChild(doc.createTextNode(payload.text || ""));
    else if (name === "done") {
      renderAnswer(payload.answer || "");
      card.setAttribute("aria-busy", "false");
    } else if (name === "error") {
      let message = payload.message || AS.stream_failed;
      if (payload.retry_after_s) {
        message += " (" + AS.retry_after.replace("{n}", payload.retry_after_s) + ")";
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
                (env && env.error && env.error.message) ||
                  AS.stream_failed + ": HTTP " + res.status,
              );
            },
            () => {
              fail(AS.stream_failed + ": HTTP " + res.status);
            },
          );
        }
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        function pump() {
          return reader.read().then((chunk) => {
            buffer += decoder.decode(chunk.value, { stream: !chunk.done });
            let i;
            while ((i = buffer.indexOf("\n\n")) >= 0) {
              handleFrame(buffer.slice(0, i));
              buffer = buffer.slice(i + 2);
            }
            if (!chunk.done) return pump();
            if (buffer.trim()) handleFrame(buffer);
            // A stream that closes without a terminal frame must not
            // leave the card "answering" forever.
            if (card.getAttribute("aria-busy") === "true") fail(AS.stream_failed);
          });
        }
        return pump();
      })
      .catch(() => {
        fail(AS.stream_failed);
      });
  });

  return { setContext };
}
