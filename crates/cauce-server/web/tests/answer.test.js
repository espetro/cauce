// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from "vitest";
import { createAnswerSession, initAnswerPage, parseSseFrame } from "../src/answer.js";

const S = {
  error_status: "answer failed",
  invalid_stream: "invalid stream",
  stream_failed: "stream failed",
  sources: "Sources",
  confidence: "confidence {n}/10",
  cached: "cached",
  related: "Related",
  complete: "Done",
  retry_after: "retry in {n}s",
};

/** The DOM shell answer.html renders while `has_query && enabled`. */
function answerShell() {
  document.body.innerHTML = `
    <div id="answer-stream" aria-busy="true"
         data-endpoint="/api/answer" data-accept="text/event-stream"
         data-headers='{"X-Cauce-Client":"ui"}'>
      <span id="answer-status"></span>
      <span id="answer-meta"></span>
      <span id="request-id"></span>
      <ol id="answer-steps"></ol>
      <p id="answer-ungrounded" hidden></p>
      <div id="answer-text"></div>
      <div id="answer-sources"></div>
      <div id="answer-related"></div>
      <p id="answer-error" hidden></p>
    </div>`;
  const $ = (id) => document.getElementById(id);
  return {
    stream: $("answer-stream"),
    status: $("answer-status"),
    meta: $("answer-meta"),
    requestId: $("request-id"),
    steps: $("answer-steps"),
    text: $("answer-text"),
    sourcesEl: $("answer-sources"),
    relatedEl: $("answer-related"),
    ungrounded: $("answer-ungrounded"),
    errorEl: $("answer-error"),
  };
}

function session(overrides = {}) {
  const refs = answerShell();
  const fetchImpl = overrides.fetchImpl || vi.fn();
  const s = createAnswerSession(refs, S, "what is rust", { fetchImpl });
  return { refs, fetchImpl, ...s };
}

describe("parseSseFrame", () => {
  it("parses event and data lines", () => {
    expect(parseSseFrame('event: delta\ndata: {"text":"hi"}')).toEqual({
      name: "delta",
      data: '{"text":"hi"}',
    });
  });

  it("defaults the event name to message", () => {
    expect(parseSseFrame('data: {"a":1}').name).toBe("message");
  });

  it("concatenates multiple data lines", () => {
    expect(parseSseFrame('data: {"a":\ndata: 1}').data).toBe('{"a":1}');
  });

  it("ignores comments and other lines", () => {
    expect(parseSseFrame(': keep-alive\ndata: x').data).toBe("x");
  });
});

describe("createAnswerSession — frame dispatch", () => {
  it("step frames append a progress item and move the status", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: step\ndata: {"label":"searching"}');
    expect(refs.steps.querySelector("li").textContent).toBe("searching");
    expect(refs.status.textContent).toBe("searching");
  });

  it("delta frames append raw text", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: delta\ndata: {"text":"hello "}');
    handleFrame('event: delta\ndata: {"text":"world"}');
    expect(refs.text.textContent).toBe("hello world");
  });

  it("sources frames render numbered cards", () => {
    const { refs, handleFrame } = session();
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://a.com/x","title":"A","snippet":"sa"}]}',
    );
    const card = refs.sourcesEl.querySelector(".source-card");
    expect(card.id).toBe("src-1");
    expect(card.querySelector(".cite-badge").textContent).toBe("[1]");
    expect(card.querySelector("a").textContent).toBe("A");
    expect(refs.sourcesEl.querySelector(".section-label").textContent).toBe("Sources");
  });

  it("done renders the answer, meta line, related questions and completes", () => {
    const { refs, handleFrame } = session();
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://a.com/x","title":"A","snippet":"s"}]}',
    );
    handleFrame(
      `event: done\ndata: ${JSON.stringify({
        answer: "Rust is a language [1].",
        ungrounded: true,
        confidence: 6,
        model: "liquid/lfm",
        cached: true,
        request_id: "0123456789abcdef",
        related_questions: ["why rust"],
      })}`,
    );
    const cite = refs.text.querySelector("a.cite");
    expect(cite.getAttribute("href")).toBe("#src-1");
    expect(cite.textContent).toBe("[1]");
    expect(refs.text.textContent).toBe("Rust is a language [1].");
    expect(refs.ungrounded.hidden).toBe(false);
    expect(refs.meta.textContent).toBe("confidence 6/10 · liquid/lfm · cached");
    expect(refs.requestId.textContent).toBe("01234567");
    const rel = refs.relatedEl.querySelector(".related-question a");
    expect(rel.getAttribute("href")).toBe("/answer?q=why%20rust");
    expect(refs.status.textContent).toBe("Done");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("done with an out-of-range citation leaves the marker as text", () => {
    const { refs, handleFrame } = session();
    handleFrame(`event: done\ndata: ${JSON.stringify({ answer: "see [9]" })}`);
    expect(refs.text.querySelector("a.cite")).toBeNull();
    expect(refs.text.textContent).toBe("see [9]");
  });

  it("error frames fail with the payload message plus retry hint", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: error\ndata: {"message":"rate limited","retry_after_s":30}');
    expect(refs.errorEl.textContent).toBe("rate limited (retry in 30s)");
    expect(refs.errorEl.hidden).toBe(false);
    expect(refs.status.textContent).toBe("answer failed");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("malformed frames fail as invalid stream; data-less frames are skipped", () => {
    const { refs, handleFrame } = session();
    handleFrame("event: delta"); // no data — skipped
    expect(refs.errorEl.hidden).toBe(true);
    handleFrame("event: delta\ndata: {nope");
    expect(refs.errorEl.textContent).toBe("invalid stream");
  });
});

describe("createAnswerSession — pump and start", () => {
  function fakeResponse(frames, ok = true, status = 200) {
    const chunks = frames.map((f) => new TextEncoder().encode(f));
    let i = 0;
    return {
      ok,
      status,
      json: () => Promise.reject(new Error("no json")),
      body: {
        getReader: () => ({
          read: () =>
            Promise.resolve(
              i < chunks.length
                ? { value: chunks[i++], done: false }
                : { value: undefined, done: true },
            ),
        }),
      },
    };
  }

  it("pumps frames across chunk boundaries and fails when no terminal frame arrives", async () => {
    const { refs, pump } = session();
    await pump(
      fakeResponse(['event: delta\nda', 'ta: {"text":"a"}\n\nevent: delta\ndata: {"text":"b"}\n\n']),
    );
    expect(refs.text.textContent).toBe("ab");
    expect(refs.errorEl.hidden).toBe(false); // stream ended still busy
    expect(refs.errorEl.textContent).toBe("stream failed");
  });

  it("start() POSTs the endpoint with the data-* contract and {q}", async () => {
    const res = fakeResponse(['event: done\ndata: {"answer":"ok"}\n\n']);
    const fetchImpl = vi.fn(() => Promise.resolve(res));
    const { refs, start } = session({ fetchImpl });
    await start();
    const [url, init] = fetchImpl.mock.calls[0];
    expect(url).toBe("/api/answer");
    expect(init.method).toBe("POST");
    expect(init.headers["Accept"]).toBe("text/event-stream");
    expect(init.headers["X-Cauce-Client"]).toBe("ui");
    expect(JSON.parse(init.body)).toEqual({ q: "what is rust" });
    expect(refs.status.textContent).toBe("Done");
  });

  it("start() surfaces the error envelope on non-2xx", async () => {
    const fetchImpl = vi.fn(() =>
      Promise.resolve({
        ok: false,
        status: 503,
        json: () => Promise.resolve({ error: { message: "ai_disabled" } }),
      }),
    );
    const { refs, start } = session({ fetchImpl });
    await start();
    expect(refs.errorEl.textContent).toBe("ai_disabled");
  });

  it("start() fails the stream on fetch rejection", async () => {
    const fetchImpl = vi.fn(() => Promise.reject(new Error("offline")));
    const { refs, start } = session({ fetchImpl });
    await start();
    expect(refs.errorEl.textContent).toBe("stream failed");
  });
});

describe("initAnswerPage gating", () => {
  it("starts the session only when shell, Q and S exist", async () => {
    const fetchImpl = vi.fn(() =>
      Promise.resolve({
        ok: true,
        body: {
          getReader: () => ({
            read: () =>
              Promise.resolve({ value: new TextEncoder().encode('event: done\ndata: {"answer":"x"}\n\n'), done: false }),
          }),
        },
      }),
    );
    // happy-dom reader needs two reads: frame then done. Patch to a 2-step reader.
    let reads = 0;
    fetchImpl.mockImplementation(() =>
      Promise.resolve({
        ok: true,
        body: {
          getReader: () => ({
            read: () =>
              Promise.resolve(
                reads++ === 0
                  ? { value: new TextEncoder().encode('event: done\ndata: {"answer":"x"}\n\n'), done: false }
                  : { value: undefined, done: true },
              ),
          }),
        },
      }),
    );
    const refs = answerShell();
    window.Q = "what is rust";
    window.S = S;
    initAnswerPage(document, { fetchImpl });
    await vi.waitFor(() => expect(refs.status.textContent).toBe("Done"));
    delete window.Q;
    delete window.S;
  });

  it("no-ops without the stream shell", () => {
    document.body.innerHTML = "";
    expect(() => initAnswerPage(document)).not.toThrow();
  });
});
