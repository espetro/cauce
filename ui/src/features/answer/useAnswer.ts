import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import { streamAnswer, type AiSource, type AnswerEvent } from "../../lib/ai";

export interface AnswerState {
  text: string;
  steps: string[];
  sources: AiSource[];
  done: boolean;
  cached: boolean;
  confidence: number;
  relatedQuestions: string[];
  error: string | null;
  stopped: boolean;
}

export const INITIAL: AnswerState = {
  text: "",
  steps: [],
  sources: [],
  done: false,
  cached: false,
  confidence: 0,
  relatedQuestions: [],
  error: null,
  stopped: false,
};

/** Pure SSE event reducer so event handling is testable without a stream. */
export function applyAnswerEvent(state: AnswerState, ev: AnswerEvent): AnswerState {
  if (ev.type === "step") {
    return { ...state, steps: [...state.steps, ev.label] };
  }
  if (ev.type === "delta") {
    return { ...state, text: state.text + ev.text };
  }
  if (ev.type === "sources") {
    return { ...state, sources: ev.sources };
  }
  if (ev.type === "done") {
    return {
      ...state,
      text: ev.answer || state.text,
      done: true,
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

  useEffect(() => () => abortRef.current?.abort(), []);

  const run = useCallback((query: string) => {
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
        setState((s) => ({
          ...s,
          done: true,
          stopped: !stoppedRef.current ? s.stopped : s.stopped,
        }));
        return;
      }
      setState((s) => ({ ...s, done: true, error: (e as Error)?.message ?? "answer failed" }));
    });
  }, []);

  const stop = useCallback(() => {
    stoppedRef.current = true;
    abortRef.current?.abort();
    setState((s) => ({ ...s, done: true, stopped: true }));
  }, []);

  return { state, run, stop };
}
