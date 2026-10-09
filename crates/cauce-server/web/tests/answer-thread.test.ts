// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// FX-04 §7.5 stream-contract checks for the SPA's `AnswerThread`
// (`src/spa/features/answer/thread.svelte.ts`): step ordering, sources
// held until `done`, stop/edit state retention, at-bottom-only
// auto-scroll, multi-turn history replay. The runes store needs no DOM
// — the scroll hooks are injected spies (vitest compiles `.svelte.ts`
// via the svelte plugin in vitest.config.js).

import { describe, expect, it, vi } from "vitest";
import {
  AnswerThread,
  type AnswerStrings,
} from "../src/spa/features/answer/thread.svelte.js";
import type { AnswerBody } from "../src/types/AnswerBody.js";

const S: AnswerStrings = {
  cached: "cached",
  complete: "answered",
  confidence: "confidence {n}/10",
  error_status: "answer failed",
  invalid_stream: "invalid stream",
  path_direct: "answered directly — no search needed",
  path_replay: "searched · {n} sources",
  path_searched: "searched {tools} · {n} sources",
  related: "related",
  retry_after: "retry after {n}s",
  sources: "sources",
  stopped: "stopped",
  stream_failed: "stream failed",
  tool_archive: "archive",
  tool_web: "web",
  ungrounded: "ungrounded",
  waiting: "answering...",
};

type Fetch = (body: AnswerBody, signal?: AbortSignal) => Promise<Response>;

/** A readable SSE body out of raw frames (each ends with `\n\n`). */
function fakeResponse(frames: string[], ok = true, status = 200): Response {
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
  } as unknown as Response;
}

/**
 * A Response whose stream never settles on its own — `read()` pends
 * until the abort signal fires (what real fetch does) or the test
 * releases it. Drives the stop-path evidence.
 */
function pendingResponse(): {
  res: Response;
  release: (frames: string[]) => void;
  wireSignal: (signal?: AbortSignal) => void;
} {
  let chunks: Uint8Array[] = [];
  let readIndex = 0;
  let waiting: ((r: { value?: Uint8Array; done: boolean }) => void) | null = null;
  let rejectWaiting: ((e: unknown) => void) | null = null;
  const res = {
    ok: true,
    status: 200,
    json: () => Promise.reject(new Error("no json")),
    body: {
      getReader: () => ({
        read: () =>
          new Promise((resolve, reject) => {
            if (readIndex < chunks.length) {
              resolve({ value: chunks[readIndex++], done: false });
              return;
            }
            waiting = resolve;
            rejectWaiting = reject;
          }),
      }),
    },
  } as unknown as Response;
  const release = (frames: string[]): void => {
    chunks = frames.map((f) => new TextEncoder().encode(f));
    const next = waiting;
    if (next && readIndex < chunks.length) {
      waiting = null;
      next({ value: chunks[readIndex++], done: false });
    } else if (next) {
      waiting = null;
      next({ value: undefined, done: true });
    }
  };
  // What real fetch does: the signal rejects the in-flight read.
  const wireSignal = (signal?: AbortSignal): void => {
    signal?.addEventListener("abort", () =>
      rejectWaiting?.(new DOMException("aborted", "AbortError")),
    );
  };
  return { res: res as Response, release, wireSignal };
}

interface ThreadDeps {
  fetchImpl?: Fetch;
  isAtBottom?: () => boolean;
  scrollToBottom?: () => void;
  onLogId?: (id: number) => void;
}

function thread(deps: ThreadDeps = {}): AnswerThread {
  return new AnswerThread(S, deps);
}

const DONE = (answer: string, extra: Record<string, unknown> = {}): string =>
  `event: done\ndata: ${JSON.stringify({ answer, ...extra })}\n\n`;

const SOURCES = `event: sources\ndata: ${JSON.stringify({
  sources: [
    { url: "https://a.com/x", title: "A", snippet: "sa" },
    { url: "https://b.com/y", title: "B", snippet: "sb" },
  ],
})}\n\n`;

async function settled(fetchImpl: Fetch): Promise<AnswerThread> {
  const t = thread({ fetchImpl });
  t.startTurn("what is rust");
  await vi.waitFor(() => expect(t.busy).toBe(false));
  return t;
}

describe("AnswerThread — §7.5 stream contract", () => {
  it("step frames land in arrival order and move the status to the last label", async () => {
    const t = await settled(() =>
      Promise.resolve(
        fakeResponse([
          'event: step\ndata: {"tool":"search_web","label":"searching web"}\n\n',
          'event: step\ndata: {"tool":"search_archive","label":"checking archive"}\n\n',
          DONE("ok"),
        ]),
      ),
    );
    const turn = t.turns[0];
    expect(turn.steps).toEqual(["searching web", "checking archive"]);
    expect(turn.toolsRun).toEqual(["search_web", "search_archive"]);
    expect(turn.status).toBe("answered");
  });

  it("a sources frame mid-stream is held — citations mount only at done", async () => {
    // Drive dispatch by hand to observe the pre-done boundary.
    const t = thread();
    t.turns.push({
      n: 1, q: "q", status: S.waiting, steps: [], toolsRun: [], deltas: "",
      html: "", pendingSources: [], sources: [], confidence: null,
      ungrounded: false, cached: false, model: "", related: [],
      requestId: "", pathText: "", pathKind: "", error: "",
      terminal: false, stopped: false,
    });
    const turn = t.turns[0];
    t.dispatch(turn, SOURCES.slice(0, -1)); // dispatch takes one frame
    expect(turn.pendingSources).toHaveLength(2);
    expect(turn.sources).toHaveLength(0); // §7.2.1 — nothing mounted yet
    t.dispatch(turn, DONE("ok [1]"));
    expect(turn.sources).toHaveLength(2);
    expect(turn.terminal).toBe(true);
  });

  it("stop aborts the pump, keeps the partial turn, and restores the composer", async () => {
    const { res, release, wireSignal } = pendingResponse();
    let signal: AbortSignal | undefined;
    const fetchImpl: Fetch = (_body, s) => {
      signal = s;
      wireSignal(s);
      return Promise.resolve(res);
    };
    const t = thread({ fetchImpl });
    t.startTurn("first question");
    await vi.waitFor(() => expect(t.busy).toBe(true));
    release(['event: delta\ndata: {"text":"partial "}\n\n']);
    await vi.waitFor(() => expect(t.turns[0].deltas).toBe("partial "));
    t.stop();
    await vi.waitFor(() => expect(t.busy).toBe(false));
    expect(signal?.aborted).toBe(true);
    const turn = t.turns[0];
    expect(turn.stopped).toBe(true);
    expect(turn.status).toBe("stopped");
    expect(turn.deltas).toBe("partial "); // state kept, §7.2.2
    expect(turn.terminal).toBe(true);
    expect(t.history).toHaveLength(0); // never replays
    expect(t.composerText).toBe("first question"); // editable again
  });

  it("auto-scrolls on new frames only while already at the bottom", async () => {
    let atBottom = true;
    const scrollToBottom = vi.fn();
    const t = thread({
      isAtBottom: () => atBottom,
      scrollToBottom,
      fetchImpl: () => new Promise<Response>(() => {}),
    });
    t.turns.push({
      n: 1, q: "q", status: S.waiting, steps: [], toolsRun: [], deltas: "",
      html: "", pendingSources: [], sources: [], confidence: null,
      ungrounded: false, cached: false, model: "", related: [],
      requestId: "", pathText: "", pathKind: "", error: "",
      terminal: false, stopped: false,
    });
    const turn = t.turns[0];
    t.dispatch(turn, 'event: delta\ndata: {"text":"a"}');
    expect(scrollToBottom).toHaveBeenCalledTimes(1); // was at bottom
    atBottom = false; // user scrolled up — content growth must not yank
    t.dispatch(turn, 'event: delta\ndata: {"text":"b"}');
    expect(scrollToBottom).toHaveBeenCalledTimes(1);
  });

  it("editLast rewinds the settled turn and its history pair", async () => {
    const t = await settled(() =>
      Promise.resolve(fakeResponse([DONE("first reply")])),
    );
    expect(t.history).toHaveLength(2);
    t.editLast();
    expect(t.turns).toHaveLength(0);
    expect(t.history).toHaveLength(0);
    expect(t.composerText).toBe("what is rust");
  });

  it("editLast refuses mid-stream and failed turns", async () => {
    const t = await settled(() =>
      Promise.resolve(
        fakeResponse(['event: error\ndata: {"message":"boom"}\n\n']),
      ),
    );
    expect(t.turns[0].error).toBe("boom");
    t.editLast();
    expect(t.turns).toHaveLength(1); // failed turn stays for context
  });
});

describe("AnswerThread — multi-turn history", () => {
  it("a follow-up appends turn 2 and replays completed pairs verbatim", async () => {
    const bodies: AnswerBody[] = [];
    const fetchImpl: Fetch = (body) => {
      bodies.push(JSON.parse(JSON.stringify(body)) as AnswerBody);
      return Promise.resolve(fakeResponse([DONE("reply " + bodies.length)]));
    };
    const t = thread({ fetchImpl });
    t.startTurn("what is rust");
    await vi.waitFor(() => expect(t.busy).toBe(false));
    t.startTurn("and the borrow checker?");
    await vi.waitFor(() => expect(t.busy).toBe(false));
    expect(t.turns).toHaveLength(2);
    expect(t.turns[1].q).toBe("and the borrow checker?");
    expect(t.turns[1].status).toBe("answered");
    expect(bodies).toHaveLength(2);
    expect(bodies[1].history).toEqual([
      { role: "user", content: "what is rust" },
      { role: "assistant", content: "reply 1" },
    ]);
    expect(bodies[0].history).toBeNull();
  });

  it("a failed turn leaves no history entry — the retry is clean", async () => {
    const bodies: AnswerBody[] = [];
    let call = 0;
    const fetchImpl: Fetch = (body) => {
      bodies.push(JSON.parse(JSON.stringify(body)) as AnswerBody);
      call += 1;
      return Promise.resolve(
        call === 1
          ? fakeResponse([DONE("reply")])
          : fakeResponse(['event: error\ndata: {"message":"rate limited"}\n\n']),
      );
    };
    const t = thread({ fetchImpl });
    t.startTurn("q1");
    await vi.waitFor(() => expect(t.busy).toBe(false));
    t.startTurn("q2");
    await vi.waitFor(() => expect(t.busy).toBe(false));
    expect(t.turns[1].error).toBe("rate limited");
    expect(t.history).toHaveLength(2); // only turn 1's pair
    expect(t.composerText).toBe("q2"); // retry path keeps the text
  });

  it("blank and mid-stream submissions are ignored", async () => {
    const fetchImpl: Fetch = () =>
      Promise.resolve(fakeResponse([DONE("ok")]));
    const t = thread({ fetchImpl });
    t.startTurn("   ");
    expect(t.turns).toHaveLength(0);
    t.startTurn("q");
    expect(t.busy).toBe(true);
    t.startTurn("ignored while busy");
    expect(t.turns).toHaveLength(1);
    await vi.waitFor(() => expect(t.busy).toBe(false));
  });
});

describe("AnswerThread — done rendering + chips", () => {
  it("a searched answer names deduped tools + cited-source count", async () => {
    const t = await settled(() =>
      Promise.resolve(
        fakeResponse([
          'event: step\ndata: {"tool":"search_web","label":"searching"}\n\n',
          'event: step\ndata: {"tool":"search_archive","label":"checking archive"}\n\n',
          'event: step\ndata: {"tool":"search_web","label":"searching again"}\n\n',
          SOURCES,
          DONE("ok", { confidence: 8 }),
        ]),
      ),
    );
    const turn = t.turns[0];
    expect(turn.pathKind).toBe("searched");
    expect(turn.pathText).toBe("searched web + archive · 2 sources");
    expect(turn.confidence).toBe(8);
  });

  it("a cached replay names the source count but no tools", async () => {
    const t = await settled(() =>
      Promise.resolve(fakeResponse([SOURCES, DONE("ok", { confidence: 9, cached: true })])),
    );
    const turn = t.turns[0];
    expect(turn.pathText).toBe("searched · 2 sources");
    expect(turn.cached).toBe(true);
  });

  it("a no-tool answer reads 'answered directly'", async () => {
    const t = await settled(() =>
      Promise.resolve(fakeResponse([DONE("4", { confidence: 10 })])),
    );
    expect(t.turns[0].pathText).toBe("answered directly — no search needed");
    expect(t.turns[0].pathKind).toBe("direct");
  });

  it("done carries html, model, related questions and the log-id hook", async () => {
    const onLogId = vi.fn();
    const t = thread({
      onLogId,
      fetchImpl: () =>
        Promise.resolve(
          fakeResponse([
            DONE("reply", {
              html: "<p>reply</p>",
              model: "liquid/lfm",
              request_id: "0123456789abcdef",
              related_questions: ["why rust"],
              log_id: 42,
            }),
          ]),
        ),
    });
    t.startTurn("q");
    await vi.waitFor(() => expect(t.busy).toBe(false));
    const turn = t.turns[0];
    expect(turn.html).toBe("<p>reply</p>");
    expect(turn.model).toBe("liquid/lfm");
    expect(turn.requestId).toBe("0123456789abcdef");
    expect(turn.related).toEqual(["why rust"]);
    expect(onLogId).toHaveBeenCalledWith(42);
  });

  it("error frames carry the message plus the retry hint", async () => {
    const t = await settled(() =>
      Promise.resolve(
        fakeResponse(['event: error\ndata: {"message":"limited","retry_after_s":30}\n\n']),
      ),
    );
    expect(t.turns[0].error).toBe("limited (retry after 30s)");
    expect(t.turns[0].status).toBe("answer failed");
  });

  it("a stream that closes without a terminal frame fails the turn", async () => {
    const t = await settled(() =>
      Promise.resolve(fakeResponse(['event: delta\ndata: {"text":"partial"}\n\n'])),
    );
    expect(t.turns[0].error).toBe("stream failed");
    expect(t.turns[0].terminal).toBe(true);
  });
});
