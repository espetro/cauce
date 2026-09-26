// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { afterEach, describe, expect, it, vi } from "vitest";
import { initAssist } from "../src/assist.js";

const AS = {
  stream_failed: "answer stream failed",
  invalid_stream: "unreadable answer stream",
  retry_after: "retry after {n}s",
};

/** The DOM shell page.html renders inside `{% if assist %}`. */
function assistShell({ streaming = false } = {}) {
  document.body.innerHTML = `
    <section id="assist" class="assist" data-q="tokyo weather">
      <button type="button" id="assist-btn"${streaming ? " disabled" : ""}>Assist</button>
      <div id="assist-card" hidden aria-busy="false">
        <div id="assist-text"></div>
        <div id="assist-sources"></div>
        <p id="assist-error" hidden></p>
      </div>
    </section>`;
  return {
    section: document.getElementById("assist"),
    btn: document.getElementById("assist-btn"),
    card: document.getElementById("assist-card"),
    text: document.getElementById("assist-text"),
    sources: document.getElementById("assist-sources"),
    err: document.getElementById("assist-error"),
  };
}

const ROWS = [
  { url: "https://a.example.com/x", title: "A", snippet: "sa", engine: "replay" },
  { url: "https://b.example.com/y", title: "B", snippet: "sb", engine: "replay" },
];

/** A fetch stub whose body is a scripted reader (no ReadableStream needed). */
function fetchWithChunks(chunks) {
  const encoded = chunks.map((s) => new TextEncoder().encode(s));
  let i = 0;
  return vi.fn(() =>
    Promise.resolve({
      ok: true,
      body: {
        getReader: () => ({
          read: () =>
            Promise.resolve(
              i < encoded.length
                ? { value: encoded[i++], done: false }
                : { value: undefined, done: true },
            ),
        }),
      },
    }),
  );
}

function fetchError(status, envelope) {
  return vi.fn(() =>
    Promise.resolve({ ok: false, status, json: () => Promise.resolve(envelope) }),
  );
}

async function flush() {
  // Let the fetch/pump promise chain settle.
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("initAssist", () => {
  it("is a no-op without the assist section", () => {
    document.body.innerHTML = "<p>plain page</p>";
    expect(initAssist(document, AS, ROWS, vi.fn())).toBeNull();
  });

  it("is a no-op without the AS bundle", () => {
    assistShell();
    expect(initAssist(document, undefined, ROWS, vi.fn())).toBeNull();
  });

  it("posts q + context_results on click and opens the card", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const [url, init] = fetchImpl.mock.calls[0];
    expect(url).toBe("/api/answer");
    expect(init.method).toBe("POST");
    expect(init.headers.Accept).toBe("text/event-stream");
    expect(JSON.parse(init.body)).toEqual({ q: "tokyo weather", context_results: ROWS });
    expect(refs.btn.hidden).toBe(true);
    expect(refs.btn.getAttribute("aria-expanded")).toBe("true");
    expect(refs.card.hidden).toBe(false);
    expect(refs.card.getAttribute("aria-busy")).toBe("true");
  });

  it("fires only once — a second click does not re-POST", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    refs.btn.click();
    await flush();
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it("does not fire with empty context", async () => {
    const refs = assistShell({ streaming: true });
    const fetchImpl = fetchWithChunks([]);
    initAssist(document, AS, [], fetchImpl);
    refs.btn.click();
    await flush();
    expect(fetchImpl).not.toHaveBeenCalled();
    expect(refs.card.hidden).toBe(true);
  });
});

describe("setContext (meta-frame wiring)", () => {
  it("arms the disabled streaming trigger when the merged order has rows", () => {
    const refs = assistShell({ streaming: true });
    const assist = initAssist(document, AS, [], vi.fn());
    assist.setContext(ROWS);
    expect(refs.btn.disabled).toBe(false);
  });

  it("hides the section when the merged order is empty", () => {
    const refs = assistShell({ streaming: true });
    const assist = initAssist(document, AS, [], vi.fn());
    assist.setContext([]);
    expect(refs.section.hidden).toBe(true);
    expect(refs.btn.disabled).toBe(true);
  });

  it("caps the context at 10 rows", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([]);
    const assist = initAssist(document, AS, [], fetchImpl);
    assist.setContext(Array.from({ length: 14 }, (_, i) => ({ url: "https://x.example/" + i })));
    refs.btn.click();
    await flush();
    expect(JSON.parse(fetchImpl.mock.calls[0][1].body).context_results).toHaveLength(10);
  });
});

describe("assist SSE frames", () => {
  it("renders source chips on the sources frame before answer text", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([
      'event: sources\ndata: {"sources":[{"url":"https://a.example.com/x","title":"A"}]}\n\n',
      'event: delta\ndata: {"text":"partial"}\n\n',
    ]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    const chip = refs.sources.querySelector("a.assist-chip");
    expect(chip).not.toBeNull();
    expect(chip.id).toBe("asrc-1");
    expect(chip.href).toBe("https://a.example.com/x");
    expect(chip.querySelector("span").textContent).toBe("a.example.com");
    expect(refs.text.textContent).toBe("partial");
  });

  it("falls back to the title when a source URL does not parse", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([
      'event: sources\ndata: {"sources":[{"url":"not a url","title":"Plain"}]}\n\n',
    ]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.sources.querySelector("a.assist-chip").textContent).toBe("Plain");
  });

  it("linkifies [n] markers in the done answer to the numbered chips", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([
      'event: sources\ndata: {"sources":[{"url":"https://a.example.com/x"},{"url":"https://b.example.com/y"}]}\n\n',
      'event: delta\ndata: {"text":"streaming text [1]"}\n\n',
      'event: done\ndata: {"answer":"final [1] cites [2] and [9]"}\n\n',
    ]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    // done re-renders the whole answer (replacing delta text).
    expect(refs.text.textContent).toBe("final [1] cites [2] and [9]");
    const cites = refs.text.querySelectorAll("a.cite");
    expect(cites).toHaveLength(2);
    expect(cites[0].getAttribute("href")).toBe("#asrc-1");
    expect(cites[1].getAttribute("href")).toBe("#asrc-2");
    // [9] has no chip: left as plain text.
    expect(refs.card.getAttribute("aria-busy")).toBe("false");
  });

  it("splits frames on \\n\\n boundaries even mid-chunk", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([
      'event: sources\ndata: {"sources":[]}\n\nevent: delta\nda',
      'ta: {"text":"hi"}\n\n',
    ]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.text.textContent).toBe("hi");
  });

  it("shows the error frame message with the retry_after suffix", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks([
      'event: error\ndata: {"message":"provider down","retry_after_s":30}\n\n',
    ]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.hidden).toBe(false);
    expect(refs.err.textContent).toBe("provider down (retry after 30s)");
    expect(refs.card.getAttribute("aria-busy")).toBe("false");
  });

  it("fails the card when a stream closes without a terminal frame", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks(['event: delta\ndata: {"text":"half"}\n\n']);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.hidden).toBe(false);
    expect(refs.err.textContent).toBe(AS.stream_failed);
  });

  it("shows invalid_stream on unparseable frame data", async () => {
    const refs = assistShell();
    const fetchImpl = fetchWithChunks(["event: delta\ndata: {oops\n\n"]);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.textContent).toBe(AS.invalid_stream);
  });
});

describe("assist fetch failures", () => {
  it("surfaces the envelope's error.message on non-2xx", async () => {
    const refs = assistShell();
    const fetchImpl = fetchError(429, { error: { message: "rate limited" } });
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.textContent).toBe("rate limited");
  });

  it("falls back to 'stream_failed: HTTP <status>' without an envelope", async () => {
    const refs = assistShell();
    const fetchImpl = fetchError(502, undefined);
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.textContent).toBe("answer stream failed: HTTP 502");
  });

  it("fails the card when fetch rejects", async () => {
    const refs = assistShell();
    const fetchImpl = vi.fn(() => Promise.reject(new Error("offline")));
    initAssist(document, AS, ROWS, fetchImpl);
    refs.btn.click();
    await flush();
    expect(refs.err.textContent).toBe(AS.stream_failed);
  });
});
