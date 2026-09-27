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
  path_direct: "answered directly — no search needed",
  path_searched: "searched {tools} · {n} sources",
  path_replay: "searched · {n} sources",
  tool_web: "web",
  tool_archive: "archive",
};

/** The per-turn inner markup shared by the SSR shell and the template. */
function turnMarkup(ids) {
  const id = (name) => (ids ? `id="${name}"` : "");
  return `
    <section class="answer-turn">
      <p class="turn-q"></p>
      <div class="meta">
        <span ${id("answer-status")} class="answer-status"></span>
        <span ${id("answer-path")} class="meta-chip answer-path" hidden></span>
        <span ${id("answer-confidence")} class="meta-chip answer-confidence" hidden></span>
        <span ${id("answer-ungrounded-badge")} class="meta-chip warn answer-ungrounded-badge" hidden></span>
        <span ${id("answer-meta")} class="answer-meta"></span>
        <span ${id("request-id")} class="request-id"></span>
      </div>
      <ol ${id("answer-steps")} class="answer-steps"></ol>
      <p ${id("answer-ungrounded")} class="ungrounded" role="note" hidden></p>
      <div ${id("answer-text")} class="answer-text"></div>
      <div ${id("answer-sources")} class="answer-sources"></div>
      <div ${id("answer-related")} class="answer-related"></div>
      <p ${id("answer-error")} class="field-error answer-error" role="alert" hidden></p>
    </section>`;
}

/** The DOM shell answer.html renders while `has_query && enabled`. */
function answerShell() {
  document.body.innerHTML = `
    <div id="answer-stream" aria-busy="true"
         data-endpoint="/api/answer" data-accept="text/event-stream"
         data-headers='{"X-Cauce-Client":"ui"}'>
      ${turnMarkup(true)}
    </div>
    <template id="answer-turn-tpl">${turnMarkup(false)}</template>
    <form id="answer-followup" hidden>
      <input type="search" id="followup-q">
      <button type="submit">ask</button>
    </form>`;
  const $ = (id) => document.getElementById(id);
  return {
    stream: $("answer-stream"),
    turnTpl: $("answer-turn-tpl"),
    followupForm: $("answer-followup"),
    followupInput: $("followup-q"),
    status: $("answer-status"),
    meta: $("answer-meta"),
    requestId: $("request-id"),
    steps: $("answer-steps"),
    text: $("answer-text"),
    sourcesEl: $("answer-sources"),
    relatedEl: $("answer-related"),
    ungrounded: $("answer-ungrounded"),
    ungroundedBadge: $("answer-ungrounded-badge"),
    pathEl: $("answer-path"),
    confEl: $("answer-confidence"),
    errorEl: $("answer-error"),
  };
}

function session(overrides = {}) {
  const refs = answerShell();
  const fetchImpl = overrides.fetchImpl || vi.fn();
  const s = createAnswerSession(
    {
      stream: refs.stream,
      turnTpl: refs.turnTpl,
      followupForm: refs.followupForm,
      followupInput: refs.followupInput,
    },
    S,
    "what is rust",
    { fetchImpl },
  );
  return { refs, fetchImpl, ...s };
}

/** A readable SSE body out of raw frames (each ends with `\n\n`). */
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

/** Fire the follow-up form like a user submit (happy-dom). */
function submitFollowup(refs, value) {
  refs.followupInput.value = value;
  refs.followupForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
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
    expect(card.id).toBe("src-1-1");
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
    expect(cite.getAttribute("href")).toBe("#src-1-1");
    expect(cite.textContent).toBe("[1]");
    expect(refs.text.textContent).toBe("Rust is a language [1].");
    expect(refs.ungrounded.hidden).toBe(false);
    // W7-03 chips carry grounded/confidence; the meta line keeps
    // model + cache state.
    expect(refs.ungroundedBadge.hidden).toBe(false);
    expect(refs.confEl.textContent).toBe("confidence 6/10");
    expect(refs.confEl.dataset.confidence).toBe("6");
    expect(refs.meta.textContent).toBe("liquid/lfm · cached");
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

describe("createAnswerSession — W7-03 path/confidence chips", () => {
  it("a searched answer names the deduped tools and cited-source count", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: step\ndata: {"tool":"search_web","label":"searching"}');
    handleFrame('event: step\ndata: {"tool":"search_archive","label":"checking archive"}');
    handleFrame('event: step\ndata: {"tool":"search_web","label":"searching again"}');
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://a.com/x"},{"url":"https://b.com/y"},{"url":"https://c.com/z"}]}',
    );
    handleFrame('event: done\ndata: {"answer":"ok","confidence":8}');
    expect(refs.pathEl.hidden).toBe(false);
    expect(refs.pathEl.dataset.path).toBe("searched");
    expect(refs.pathEl.textContent).toBe("searched web + archive · 3 sources");
    expect(refs.confEl.hidden).toBe(false);
    expect(refs.confEl.textContent).toBe("confidence 8/10");
  });

  it("a cached replay names the source count but no tools", () => {
    const { refs, handleFrame } = session();
    // Replays emit sources + done{cached:true} with no step frames.
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://a.com/x"},{"url":"https://b.com/y"}]}',
    );
    handleFrame('event: done\ndata: {"answer":"ok","confidence":9,"cached":true}');
    expect(refs.pathEl.dataset.path).toBe("searched");
    expect(refs.pathEl.textContent).toBe("searched · 2 sources");
    expect(refs.meta.textContent).toBe("cached");
  });

  it("a no-tool answer reads 'answered directly' and still shows confidence", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: done\ndata: {"answer":"4","confidence":10}');
    expect(refs.pathEl.dataset.path).toBe("direct");
    expect(refs.pathEl.textContent).toBe("answered directly — no search needed");
    expect(refs.confEl.textContent).toBe("confidence 10/10");
  });

  it("an ungrounded done reveals both the notice and the warn chip", () => {
    const { refs, handleFrame } = session();
    handleFrame('event: done\ndata: {"answer":"ok","ungrounded":true,"confidence":5}');
    expect(refs.ungrounded.hidden).toBe(false);
    expect(refs.ungroundedBadge.hidden).toBe(false);
  });
});

describe("createAnswerSession — W7-04 thread turns", () => {
  it("done reveals the follow-up form; submitting it starts a cloned turn with history", async () => {
    const res = () => fakeResponse(['event: done\ndata: {"answer":"reply"}\n\n']);
    const fetchImpl = vi.fn(() => Promise.resolve(res()));
    const { refs, start } = session({ fetchImpl });
    await start();
    expect(refs.followupForm.hidden).toBe(false);
    expect(refs.stream.querySelectorAll(".answer-turn")).toHaveLength(1);

    submitFollowup(refs, "and borrow checker?");
    await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(2));
    await vi.waitFor(() =>
      expect(refs.stream.querySelectorAll(".answer-turn")).toHaveLength(2),
    );
    const turns = refs.stream.querySelectorAll(".answer-turn");
    expect(turns[1].dataset.turn).toBe("2");
    expect(turns[1].querySelector(".turn-q").textContent).toBe("and borrow checker?");
    // The second POST replays the completed first turn verbatim.
    expect(JSON.parse(fetchImpl.mock.calls[1][1].body)).toEqual({
      q: "and borrow checker?",
      history: [
        { role: "user", content: "what is rust" },
        { role: "assistant", content: "reply" },
      ],
    });
    await vi.waitFor(() =>
      expect(turns[1].querySelector(".answer-status").textContent).toBe("Done"),
    );
    // The input cleared for the next question and stays live.
    expect(refs.followupInput.value).toBe("");
    expect(refs.followupInput.disabled).toBe(false);
  });

  it("per-turn sources anchor [n] citations to that turn's own list", async () => {
    // fetch stays pending: frames are driven by hand this test.
    const fetchImpl = vi.fn(() => new Promise(() => {}));
    const { refs, handleFrame, startTurn } = session({ fetchImpl });
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://a.com/x","title":"A"}]}',
    );
    handleFrame('event: done\ndata: {"answer":"first [1]"}');
    startTurn("second question");
    const turns = refs.stream.querySelectorAll(".answer-turn");
    handleFrame(
      'event: sources\ndata: {"sources":[{"url":"https://b.com/y","title":"B"}]}',
    );
    handleFrame('event: done\ndata: {"answer":"second [1]"}');
    // Same [1] marker, different anchors: turn-local source lists.
    const c1 = turns[0].querySelector("a.cite");
    const c2 = turns[1].querySelector("a.cite");
    expect(c1.getAttribute("href")).toBe("#src-1-1");
    expect(c2.getAttribute("href")).toBe("#src-2-1");
    expect(document.getElementById("src-1-1").textContent).toContain("A");
    expect(document.getElementById("src-2-1").textContent).toContain("B");
  });

  it("a failed turn never enters history — the follow-up retries without it", async () => {
    const fetchImpl = vi.fn(() => new Promise(() => {}));
    const { refs, handleFrame, startTurn, history } = session({ fetchImpl });
    handleFrame('event: done\ndata: {"answer":"reply"}');
    expect(history).toHaveLength(2);
    startTurn("flaky follow-up");
    handleFrame('event: error\ndata: {"message":"rate limited"}');
    // Failed turn: no history entry, follow-up stays usable for a retry.
    expect(history).toHaveLength(2);
    expect(refs.followupForm.hidden).toBe(false);
    const turns = refs.stream.querySelectorAll(".answer-turn");
    expect(turns[1].querySelector(".answer-error").textContent).toBe("rate limited");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("blank and mid-stream submissions are ignored", async () => {
    const fetchImpl = vi.fn(() => new Promise(() => {})); // stream never settles
    const { refs, start } = session({ fetchImpl });
    start(); // in-flight for the rest of the test
    submitFollowup(refs, "   ");
    submitFollowup(refs, "while busy");
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(refs.stream.querySelectorAll(".answer-turn")).toHaveLength(1);
  });
});

describe("createAnswerSession — pump and start", () => {
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
