import type { AnswerState } from "./useAnswer";
import { MarkdownLite } from "./MarkdownLite";
import { SourceCard } from "./SourceCard";
import { Empty } from "../../components/Header";

interface Props {
  query: string;
  state: AnswerState;
  onStop: () => void;
  onRetry: () => void;
  onAskRelated: (q: string) => void;
  onViewClassic: () => void;
}

/** AI mode surface: answer, tool steps, sources row, related questions. */
export function AnswerView({ query, state, onStop, onRetry, onAskRelated, onViewClassic }: Props) {
  const { text, steps, sources, status, cached, error, relatedQuestions } = state;
  const streaming = status === "idle" || status === "streaming";
  const done = status === "done" || status === "stopped" || status === "error";
  const stopped = status === "stopped";
  const emptySources = done && !error && sources.length === 0 && !text;

  return (
    <div class="pt-2 flex flex-col gap-5 animate-in fade-in slide-in-from-bottom-2 duration-300">
      <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 class="text-xs font-semibold tracking-widest uppercase opacity-60">answer</h2>
        {cached && <span class="badge badge-ghost badge-xs">from cache</span>}
        {streaming && <span class="text-xs opacity-50">streaming…</span>}
        <span class="ml-auto flex gap-2">
          {streaming && (
            <button type="button" class="btn btn-ghost btn-xs" onClick={onStop}>
              stop
            </button>
          )}
          {done && (
            <button type="button" class="btn btn-ghost btn-xs" onClick={onViewClassic}>
              view classic
            </button>
          )}
        </span>
      </div>

      {steps.length > 0 && (
        <ul class="text-[13px] opacity-70 space-y-1" aria-live="polite">
          {steps.map((s, i) => (
            <li
              key={i}
              class="flex items-center gap-2 animate-in fade-in slide-in-from-bottom-2 duration-300"
            >
              {!done || i < steps.length - 1 ? (
                <span class="loading loading-spinner loading-xs" />
              ) : (
                <span aria-hidden="true">·</span>
              )}
              <span>{s}</span>
            </li>
          ))}
        </ul>
      )}

      {error ? (
        <div class="text-sm flex flex-col gap-2">
          {text && (
            <div class="mb-1 opacity-80">
              <MarkdownLite text={text} />
            </div>
          )}
          <div role="alert" class="alert alert-error animate-in fade-in zoom-in-95 duration-300">
            <span>stream interrupted - {error}</span>
          </div>
          <div class="flex gap-2">
            <button type="button" class="btn btn-sm" onClick={onRetry}>
              retry
            </button>
            <button type="button" class="btn btn-ghost btn-sm" onClick={onViewClassic}>
              switch to classic results
            </button>
          </div>
        </div>
      ) : emptySources ? (
        <div class="text-sm animate-in fade-in zoom-in-95 duration-300">
          <p class="opacity-60 mb-2">no sources found for this query - try fewer words, or</p>
          <button type="button" class="btn btn-sm" onClick={onViewClassic}>
            switch to classic results
          </button>
        </div>
      ) : text ? (
        <div>
          <MarkdownLite text={text} />
          {streaming && (
            <span class="animate-pulse font-mono" aria-hidden="true">
              ▌
            </span>
          )}
          {stopped && <p class="text-xs opacity-50 mt-1">stopped</p>}
        </div>
      ) : streaming && steps.length === 0 ? (
        <div class="flex flex-col gap-3 skeleton-shimmer" aria-busy="true">
          <div class="skeleton h-4 w-11/12" />
          <div class="skeleton h-4 w-full" />
          <div class="skeleton h-4 w-3/4" />
          <div class="flex gap-2 overflow-hidden">
            {[0, 1, 2].map((i) => (
              <div key={i} class="skeleton h-16 w-44 shrink-0" />
            ))}
          </div>
        </div>
      ) : null}

      {sources.length > 0 && (
        <div>
          <h2 class="text-xs font-semibold tracking-widest uppercase opacity-60 mb-2">sources</h2>
          <div class="flex gap-2 overflow-x-auto pb-2 snap-x -mx-4 px-4">
            {sources.map((s, i) => (
              <div
                key={s.url}
                class="animate-in fade-in slide-in-from-bottom-2 duration-300"
                style={{ animationDelay: `${i * 40}ms` }}
              >
                <SourceCard source={s} n={i + 1} queryHash={query} />
              </div>
            ))}
          </div>
        </div>
      )}

      {done && !error && relatedQuestions.length > 0 && (
        <div class="animate-in fade-in duration-300">
          <h2 class="text-xs font-semibold tracking-widest uppercase opacity-60 mb-2">related</h2>
          <ul class="space-y-1.5">
            {relatedQuestions.map((rq) => (
              <li key={rq}>
                <button
                  type="button"
                  class="text-left text-sm hover:text-primary hover:underline"
                  onClick={() => onAskRelated(rq)}
                >
                  {rq}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {!text && !error && !emptySources && done && <Empty>no answer produced</Empty>}
    </div>
  );
}
