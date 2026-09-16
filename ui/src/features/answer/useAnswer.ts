import { useCallback, useRef, useState } from "preact/hooks";
import { streamAnswer, type AiSource, type AnswerEvent } from "../../lib/ai";
import { useMountEffect } from "../../lib/useMountEffect";

export type AnswerStatus = "idle" | "streaming" | "done" | "error" | "stopped";

export interface AnswerState {
  status: AnswerStatus;
  text: string;
  steps: string[];
  sources: AiSource[];
  cached: boolean;
  confidence: number;
  relatedQuestions: string[];
  error: string | null;
}

export const INITIAL: AnswerState = {
  status: "idle",
  text: "",
  steps: [],
  sources: [],
  cached: false,
  confidence: 0,
  relatedQuestions: [],
  error: null,
};

/** Pure SSE event reducer so event handling is testable without a stream.
 * The first event transitions idle -> streaming; done/error terminalize. */
export function applyAnswerEvent(state: AnswerState, ev: AnswerEvent): AnswerState {
  if (ev.type === "step") {
    return { ...state, status: "streaming", steps: [...state.steps, ev.label] };
  }
  if (ev.type === "delta") {
    return { ...state, status: "streaming", text: state.text + ev.text };
  }
  if (ev.type === "sources") {
    return { ...state, status: "streaming", sources: ev.sources };
  }
  if (ev.type === "done") {
    return {
      ...state,
      text: ev.answer || state.text,
      status: state.status === "stopped" ? "stopped" : ev.error ? "error" : "done",
      cached: ev.cached,
      confidence: ev.confidence,
      relatedQuestions: ev.related_questions ?? [],
      error: ev.error ?? null,
    };
  }
  const _exhaustive: never = ev;
  return _exhaustive;
}

/** Owns the SSE answer stream lifecycle for one query run. */
export function useAnswer() {
  const [state, setState] = useState<AnswerState>(INITIAL);
  const abortRef = useRef<AbortController | null>(null);
  const stoppedRef = useRef(false);

  useMountEffect(function abortStreamOnUnmount() {
    return () => abortRef.current?.abort();
  });

  const run = useCallback(function runAnswerStream(query: string) {
    abortRef.current?.abort();
    const ctl = new AbortController();
    abortRef.current = ctl;
    stoppedRef.current = false;
    setState(INITIAL);

    const on = (ev: AnswerEvent) => {
      setState((s) => applyAnswerEvent(s, ev));
    };

    streamAnswer(query, on, ctl.signal).catch((e: unknown) => {
      if ((e as Error)?.name === "AbortError") {
        setState((s) => (stoppedRef.current ? s : { ...s, status: "stopped" }));
        return;
      }
      setState((s) => ({ ...s, status: "error", error: (e as Error)?.message ?? "answer failed" }));
    });
  }, []);

  const stop = useCallback(function stopAnswerStream() {
    stoppedRef.current = true;
    abortRef.current?.abort();
    setState((s) => ({ ...s, status: "stopped" }));
  }, []);

  return { state, run, stop };
}
