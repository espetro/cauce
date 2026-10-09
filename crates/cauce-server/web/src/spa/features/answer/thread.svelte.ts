/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * The `/app/answer` thread — `createAnswerSession` (`web/src/answer.ts`)
 * re-expressed as a runes class (`$state` fields like
 * `search.svelte.ts`'s `SearchPageState`). Vitest loads it through the
 * svelte plugin so the §7.5 stream-contract checks run against the
 * real store.
 *
 * Wire semantics are identical to the HTMX page: `POST /api/answer`
 * (`AnswerBody`) -> named SSE frames (`step`/`delta`/`sources`/`done`/
 * `error`); completed turns join `history` as `user`/`assistant` pairs
 * replayed verbatim on the next turn. §7.2 deltas vs HTMX: `sources`
 * are held until `done` (no citation ghosts mid-stream), an in-flight
 * turn is stoppable (`stop()` aborts the pump, keeps the partial
 * turn, and restores the question for editing), and a settled last
 * turn can be rewound (`editLast()`) so its question goes back to the
 * composer.
 */

/// <reference types="svelte" />
// `svelte/types` carries the ambient rune globals ($state, ...) — needed
// for plain tsc when tests pull this file into their program.

import { errorFrom, postAnswer } from "../../lib/api.js";
import { parseSseFrame, pumpSse, type SseResponse } from "../../lib/sse.js";
import { SA, fmt } from "../../lib/i18n.js";
import type { AnswerBody } from "../../../types/AnswerBody.js";
import type { AnswerFrame } from "../../../types/AnswerFrame.js";
import type { AnswerSource } from "../../../types/AnswerSource.js";
import type { AnswerTurn } from "../../../types/AnswerTurn.js";

type DoneFrame = Extract<AnswerFrame, { type: "done" }>;

/** The bundle the store interpolates (`SA` shape; tests inject a stub). */
export interface AnswerStrings {
  waiting: string;
  complete: string;
  error_status: string;
  confidence: string;
  cached: string;
  ungrounded: string;
  path_direct: string;
  path_searched: string;
  path_replay: string;
  tool_web: string;
  tool_archive: string;
  related: string;
  sources: string;
  retry_after: string;
  stream_failed: string;
  invalid_stream: string;
  stopped: string;
}

/** One exchange of the thread — a query bubble plus its reply state. */
export interface AnswerTurnState {
  /** 1-based position in the thread (`src-<n>-<i>` cite ids key on it). */
  n: number;
  q: string;
  /** Latest step label / terminal status word. */
  status: string;
  /** `step` frame labels in arrival order (collapsed by default, §7.2). */
  steps: string[];
  /** Tool names from `step` frames, run order — the path chip input. */
  toolsRun: string[];
  /** Accumulated `delta` text (rendered pre-wrap until `done.html`). */
  deltas: string;
  /** Server-sanitized `done.html`; presence flips the body to `.md`. */
  html: string;
  /** `sources` payloads arrive mid-stream but only mount at `done`. */
  pendingSources: AnswerSource[];
  /** The mounted source cards — set exactly once, at `done`. */
  sources: AnswerSource[];
  confidence: number | null;
  ungrounded: boolean;
  cached: boolean;
  model: string;
  related: string[];
  requestId: string;
  /** The W7-03 retrieval-path chip text (`data-path` kind). */
  pathText: string;
  pathKind: "searched" | "direct" | "";
  error: string;
  /** A `done`/`error` frame or a stop has settled this turn. */
  terminal: boolean;
  /** User-aborted mid-stream — partial state kept, never in history. */
  stopped: boolean;
}

/** Scroll + history hooks the route shell wires to the DOM. */
export interface ThreadHooks {
  /** True while the viewport is already at the bottom (§7.2.3). */
  isAtBottom?: () => boolean;
  /** Scroll the viewport to the bottom (called only when at bottom). */
  scrollToBottom?: () => void;
  /** A new turn mounted — the shell scrolls it into view. */
  onBeginTurn?: (turn: AnswerTurnState) => void;
  /** `done`/`error` `log_id` — the shell `replaceState`s the URL. */
  onLogId?: (id: number) => void;
}

export interface ThreadDeps extends ThreadHooks {
  fetchImpl?: typeof postAnswer;
  now?: () => number;
}

function newTurn(n: number, q: string, S: AnswerStrings): AnswerTurnState {
  return {
    n,
    q,
    status: S.waiting,
    steps: [],
    toolsRun: [],
    deltas: "",
    html: "",
    pendingSources: [],
    sources: [],
    confidence: null,
    ungrounded: false,
    cached: false,
    model: "",
    related: [],
    requestId: "",
    pathText: "",
    pathKind: "",
    error: "",
    terminal: false,
    stopped: false,
  };
}

export class AnswerThread {
  turns = $state<AnswerTurnState[]>([]);
  /** Completed user/assistant pairs replayed on the next POST. */
  history = $state<AnswerTurn[]>([]);
  busy = $state(false);
  /** The question the composer should hold (restored on stop/edit). */
  composerText = $state("");

  #S: AnswerStrings;
  #fetch: typeof postAnswer;
  #hooks: ThreadHooks;
  #abort: AbortController | null = null;

  constructor(S: AnswerStrings, deps: ThreadDeps = {}) {
    this.#S = S;
    this.#fetch = deps.fetchImpl ?? postAnswer;
    this.#hooks = deps;
  }

  get current(): AnswerTurnState | null {
    return this.turns.length ? this.turns[this.turns.length - 1] : null;
  }

  /** True while the viewport is already at the bottom (§7.2.3). */
  #atBottom(): boolean {
    return this.#hooks.isAtBottom?.() ?? false;
  }

  #follow(): void {
    if (this.#atBottom()) this.#hooks.scrollToBottom?.();
  }

  /** First turn or a follow-up: append the turn, then stream it. */
  startTurn(q: string): void {
    const trimmed = q.trim();
    if (this.busy || !trimmed) return;
    const turn = newTurn(this.turns.length + 1, trimmed, this.#S);
    this.turns.push(turn);
    // $state elements come back proxied — mutations MUST go through the
    // proxy (writes to the raw object never reach the state tree).
    const proxied = this.turns[this.turns.length - 1];
    this.#hooks.onBeginTurn?.(proxied);
    void this.#stream(proxied);
  }

  /**
   * §7.2.2: interruption is a feature. Abort the in-flight pump; the
   * turn keeps its partial steps/deltas, never enters `history`, and
   * the composer takes the question back for editing/resubmit.
   */
  stop(): void {
    const turn = this.current;
    if (!this.busy || !turn || turn.terminal) return;
    turn.stopped = true;
    this.#abort?.abort();
  }

  /**
   * §7.2.2 edit: rewind the most recent settled turn — remove it and
   * its history pair, restore its question to the composer. Only the
   * last turn can rewind (mid-thread surgery has no history semantics).
   */
  editLast(): void {
    const turn = this.current;
    if (this.busy || !turn || !turn.terminal || turn.error || turn.stopped) return;
    this.turns.pop();
    this.history.pop();
    this.history.pop();
    this.composerText = turn.q;
  }

  /**
   * A stop leaves the turn's question as the composer text — same as
   * a failure (the input keeps what the user typed).
   */
  #settleStopped(turn: AnswerTurnState): void {
    turn.status = this.#S.stopped;
    turn.terminal = true;
    this.busy = false;
    this.#abort = null;
    this.composerText = turn.q;
  }

  #fail(turn: AnswerTurnState, message: string): void {
    turn.error = message;
    turn.status = this.#S.error_status;
    turn.terminal = true;
    this.busy = false;
    this.#abort = null;
    // A failed turn leaves no history entry — the composer keeps the
    // question so the follow-up box doubles as the retry path.
    this.composerText = turn.q;
  }

  #renderDone(turn: AnswerTurnState, done: DoneFrame): void {
    turn.html = done.html || "";
    turn.ungrounded = !!done.ungrounded;
    // §7.2.1: citations + sources mount only at `done`.
    turn.sources = turn.pendingSources;
    // W7-03 retrieval path: tools that ran + cited-source count; a
    // cached replay ran no tools this request, so it names the count.
    const tools: string[] = [];
    for (const t of turn.toolsRun) {
      const word =
        t === "search_web"
          ? this.#S.tool_web
          : t === "search_archive"
            ? this.#S.tool_archive
            : t;
      if (!tools.includes(word)) tools.push(word);
    }
    if (tools.length) {
      turn.pathKind = "searched";
      turn.pathText = fmt(this.#S.path_searched, {
        tools: tools.join(" + "),
        n: turn.sources.length,
      });
    } else if (turn.sources.length) {
      turn.pathKind = "searched";
      turn.pathText = fmt(this.#S.path_replay, { n: turn.sources.length });
    } else {
      turn.pathKind = "direct";
      turn.pathText = this.#S.path_direct;
    }
    turn.confidence = done.confidence;
    turn.cached = done.cached;
    turn.model = done.model || "";
    turn.related = done.related_questions || [];
    turn.requestId = done.request_id || "";
    turn.status = this.#S.complete;
    turn.terminal = true;
    // #254: swap `?q=` for `/answer/{id}` so reload/back re-read the
    // stored render instead of re-running the loop.
    if (done.log_id != null) this.#hooks.onLogId?.(done.log_id);
    this.history.push({ role: "user", content: turn.q });
    this.history.push({ role: "assistant", content: done.answer || "" });
    this.composerText = "";
    this.busy = false;
    this.#abort = null;
  }

  /** Dispatch one raw SSE frame (text between `\n\n` delimiters). */
  dispatch(turn: AnswerTurnState, raw: string): void {
    const { name, data } = parseSseFrame(raw);
    if (!data) return;
    let payload: AnswerFrame;
    try {
      payload = JSON.parse(data) as AnswerFrame;
    } catch {
      return this.#fail(turn, this.#S.invalid_stream);
    }
    if (name === "step") {
      const frame = payload as Extract<AnswerFrame, { type: "step" }>;
      const label = frame.label || frame.query || frame.tool;
      turn.steps.push(label);
      turn.status = label;
      if (frame.tool) turn.toolsRun.push(frame.tool);
    } else if (name === "delta") {
      turn.deltas += (payload as Extract<AnswerFrame, { type: "delta" }>).text;
    } else if (name === "sources") {
      // Held, not mounted — `done` reveals them (§7.2.1).
      turn.pendingSources = (
        payload as Extract<AnswerFrame, { type: "sources" }>
      ).sources;
    } else if (name === "done") {
      this.#renderDone(turn, payload as DoneFrame);
    } else if (name === "error") {
      const frame = payload as Extract<AnswerFrame, { type: "error" }>;
      let message = frame.message || this.#S.stream_failed;
      if (frame.retry_after_s) {
        message += " (" + fmt(this.#S.retry_after, { n: frame.retry_after_s }) + ")";
      }
      if (frame.log_id != null) this.#hooks.onLogId?.(frame.log_id);
      this.#fail(turn, message);
    }
    this.#follow();
  }

  /** Stream `res`'s frames into `turn`; a non-terminal close fails. */
  async pump(turn: AnswerTurnState, res: SseResponse): Promise<void> {
    if (!res.body) return this.#fail(turn, this.#S.stream_failed);
    try {
      await pumpSse(res, (raw) => this.dispatch(turn, raw));
    } catch (e) {
      // AbortError surfaces here on stop() — the turn is already marked.
      if (turn.stopped) return this.#settleStopped(turn);
      return this.#fail(turn, this.#S.stream_failed);
    }
    if (turn.stopped) return this.#settleStopped(turn);
    // The stream closed without a terminal frame: surface that instead
    // of leaving the turn "answering..." forever.
    if (!turn.terminal) this.#fail(turn, this.#S.stream_failed);
  }

  /** POST `/api/answer` and pump the frames; JSON envelope on non-200. */
  async #stream(turn: AnswerTurnState): Promise<void> {
    this.busy = true;
    this.#abort = new AbortController();
    const body: AnswerBody = {
      q: turn.q,
      context_results: null,
      history: this.history.length ? [...this.history] : null,
    };
    try {
      const res = await this.#fetch(body, this.#abort.signal);
      if (!res.ok) {
        const e = await errorFrom(res);
        return this.#fail(turn, e.message.startsWith("HTTP ") ? this.#S.stream_failed + ": " + e.message : e.message);
      }
      await this.pump(turn, res);
    } catch {
      if (turn.stopped) return this.#settleStopped(turn);
      this.#fail(turn, this.#S.stream_failed);
    }
  }

  /** Route teardown — abort any in-flight stream. */
  dispose(): void {
    this.#abort?.abort();
    this.#abort = null;
  }
}

/** The default-wired thread (SA bundle + real `/api/answer` POST). */
export function createAnswerThread(deps: ThreadDeps = {}): AnswerThread {
  return new AnswerThread(SA, deps);
}
