import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { WindowVirtualizer, type WindowVirtualizerHandle } from "virtua";
import { useAiAvailable, usePageTitle } from "../components/Header";
import { useModels } from "../components/ModeSegments";
import { recordClick } from "../lib/api";
import { toast } from "../components/Toasts";
import { ResultCard } from "../features/search/ResultCard";
import { useSearch, cachedAgeOf, isCacheHit, metaLine } from "../features/search";
import { searchUrl } from "../features/search/pager";
import { fmtDur } from "../lib/format";
import { AnswerView } from "../features/answer/AnswerView";
import { useAnswer } from "../features/answer/useAnswer";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";
type Mode = "traditional" | "ai";

export default function SearchRoute() {
  const { query, route } = useLocation();
  const q = String(query?.q ?? "");
  // `p` is deprecated (continuous scroll): accepted in deep links, ignored.
  const urlMode = query?.mode === "ai" ? "ai" : "traditional";
  usePageTitle(q || "search");

  const aiAvailable = useAiAvailable();
  const { models, error: modelsError } = useModels();
  const [input, setInput] = useState(q);
  const [mode, setMode] = useState<Mode>(urlMode);
  const { state, run, loadMore, refresh } = useSearch();
  const answer = useAnswer();
  const virtuaRef = useRef<WindowVirtualizerHandle>(null);

  useEffect(() => {
    localStorage.setItem(MODE_KEY, mode);
  }, [mode]);

  // AI mode is unavailable: ?mode=ai URLs stay on classic results (no redirect)
  // with a small inline notice; the disabled toggle communicates why.
  const aiModeBlocked = mode === "ai" && aiAvailable === false;
  const effectiveMode: Mode = aiModeBlocked ? "traditional" : mode;

  // strip deprecated `p` from the canonical url (deep links still render)
  useEffect(() => {
    if (typeof query?.p === "string") {
      const sp = new URLSearchParams(window.location.search);
      sp.delete("p");
      const qs = sp.toString();
      route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query?.p]);

  useEffect(() => {
    setInput(q);
    if (q && effectiveMode === "traditional") run(q);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q]);

  const maybeLoadMore = useCallback(() => {
    // continuous scroll: fetch the next page when the user is within ~2
    // item-heights of the list end; refires for every page until MAX_PAGES
    const v = virtuaRef.current;
    const n = state.results.length;
    if (!v || n === 0) return;
    const last = n - 1;
    const itemH = v.getItemSize(last) || 140;
    const listEnd = v.getItemOffset(last) + itemH;
    const remain = listEnd - (window.scrollY + v.viewportSize);
    if (remain < 2 * itemH) loadMore(q);
  }, [state.results.length, loadMore, q]);

  // If page 1 fits the viewport, no scroll event ever fires: re-check after
  // results arrive so short first pages keep loading. Virtualizer measures
  // asynchronously, so defer one tick; loadMore guards re-entrancy itself.
  useEffect(() => {
    if (state.results.length > 0 && !state.loadingMore) {
      const t = setTimeout(maybeLoadMore, 0);
      return () => clearTimeout(t);
    }
  }, [state.results.length, state.loadingMore, maybeLoadMore]);

  // canonical url: ai mode is expressed via &mode=ai; preserve other params
  useEffect(() => {
    if (urlMode !== mode) {
      const extra: Record<string, string> = {};
      if (typeof query?.settings === "string") extra.settings = query.settings;
      route(searchUrl({ q, mode, extra }), true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, urlMode]);

  useEffect(() => {
    if (q && effectiveMode === "ai") answer.run(q);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, effectiveMode]);

  const submit = (raw: string) => {
    const t = raw.trim();
    if (!t) return;
    const extra: Record<string, string> = {};
    if (typeof query?.settings === "string") extra.settings = query.settings;
    route(searchUrl({ q: t, mode, extra }));
  };

  const askAi = useCallback(
    (query: string) => {
      setMode("ai");
      route(`/search?q=${encodeURIComponent(query)}&mode=ai`);
    },
    [route],
  );

  const viewClassic = useCallback(() => {
    setMode("traditional");
    route(`/search?q=${encodeURIComponent(q)}`);
  }, [route, q]);

  const { payload, loading, error, results } = state;
  const qHash = payload?._q_hash ?? "";

  return (
    <div class="w-full max-w-[652px] mx-auto px-4 pb-16">
      <div class="pt-4 flex flex-col gap-3">
        <SearchBox
          value={input}
          onInput={setInput}
          onSubmit={submit}
          busy={
            effectiveMode === "ai"
              ? answer.state.status === "idle" || answer.state.status === "streaming"
              : loading
          }
          size="md"
          mode={mode}
          onModeChange={setMode}
          aiAvailable={aiAvailable}
          models={models}
          modelsError={modelsError}
        />
        {aiModeBlocked && (
          <p class="text-xs opacity-60 mt-1" role="note">
            AI mode is not configured - set a model in settings
          </p>
        )}
        {effectiveMode === "traditional" && results.length > 0 && payload && (
          <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]">
            <span class="opacity-60">
              {metaLine(payload, results.length) || (loading ? "searching…" : "")}
            </span>
            {isCacheHit(payload) && (
              <span class="tooltip" data-tip="Actually search the web (refreshes this cache entry)">
                <button
                  type="button"
                  class="badge badge-sm badge-ghost cursor-pointer"
                  aria-label="cached result: click to refresh from the web"
                  onClick={() => {
                    refresh(q);
                    toast("success", "refreshed from the web");
                  }}
                >
                  cached
                  {(() => {
                    const age = cachedAgeOf(payload);
                    return age != null ? ` · ${fmtDur(age)} old` : "";
                  })()}
                </button>
              </span>
            )}
            {payload && (
              <span class="flex gap-2 ml-auto">
                <button
                  type="button"
                  class="btn btn-ghost btn-xs"
                  onClick={() => navigator.clipboard?.writeText(window.location.href)}
                >
                  copy link
                </button>
                <button
                  type="button"
                  class="btn btn-ghost btn-xs"
                  onClick={() =>
                    navigator.clipboard?.writeText(
                      JSON.stringify({
                        requestId: payload.requestId,
                        results,
                        costDollars: payload.costDollars,
                      }),
                    )
                  }
                >
                  copy json
                </button>
              </span>
            )}
          </div>
        )}
      </div>

      {effectiveMode === "ai" ? (
        <AnswerView
          query={q}
          state={answer.state}
          onStop={answer.stop}
          onRetry={() => answer.run(q)}
          onAskRelated={askAi}
          onViewClassic={viewClassic}
        />
      ) : (
        <>
          {loading && (
            <div class="py-6 flex flex-col divide-y divide-base-300" aria-busy="true">
              {[0, 1, 2, 3].map((i) => (
                <div key={i} class="py-3 flex flex-col gap-1.5">
                  <div class="flex items-center gap-2">
                    <div class="skeleton size-4 rounded-sm" />
                    <div class="skeleton h-3 w-28" />
                  </div>
                  <div class="skeleton h-5 w-2/3" />
                  <div class="skeleton h-3.5 w-full" />
                  <div class="skeleton h-3.5 w-11/12" />
                </div>
              ))}
            </div>
          )}

          {!loading && error && (
            <div class="py-6 flex flex-col gap-3 animate-in fade-in zoom-in-95 duration-300">
              {error.kind === "rate_limited" ? (
                <div role="alert" class="alert alert-warning text-sm">
                  <span>search backend rate-limited, retry shortly</span>
                </div>
              ) : (
                <div role="alert" class="alert alert-error text-sm">
                  <span>search backend failed</span>
                </div>
              )}
              <div class="flex gap-2">
                <button type="button" class="btn btn-sm" onClick={() => run(q)}>
                  retry
                </button>
                {aiAvailable === true && (
                  <button type="button" class="btn btn-ghost btn-sm" onClick={() => askAi(q)}>
                    ask AI instead
                  </button>
                )}
              </div>
            </div>
          )}

          {!loading && !error && payload && results.length === 0 && (
            <div class="py-10 text-sm animate-in fade-in zoom-in-95 duration-300">
              <p class="opacity-60 mb-3">no results</p>
              {aiAvailable === true && (
                <button type="button" class="btn btn-sm" onClick={() => askAi(q)}>
                  ask AI instead
                </button>
              )}
            </div>
          )}

          {!loading && results.length > 0 && (
            <>
              <WindowVirtualizer ref={virtuaRef} data={results} onScroll={maybeLoadMore}>
                {(r, i) => (
                  <div
                    key={r.id || r.url}
                    class="animate-in fade-in slide-in-from-bottom-2 duration-300"
                    style={{ "--i": i, animationDelay: `calc(var(--i) * 40ms)` }}
                  >
                    <ResultCard
                      result={r}
                      queryHash={qHash}
                      onOpen={(res) =>
                        recordClick({
                          query_hash: qHash,
                          result_id: res.id || res.url,
                          url: res.url,
                          title: res.title,
                        })
                      }
                    />
                  </div>
                )}
              </WindowVirtualizer>

              {state.loadingMore && (
                <div class="py-6 flex justify-center" aria-busy="true" role="status">
                  <span class="loading loading-dots loading-sm opacity-50" aria-label="loading" />
                </div>
              )}
              {!state.hasNext && !state.loadingMore && !state.moreError && (
                <p class="py-6 text-center text-sm opacity-40">end of results</p>
              )}
              {state.moreError && (
                <div class="py-6 flex flex-col items-center gap-2 text-sm">
                  <p class="opacity-60">couldn’t load more results</p>
                  <button type="button" class="btn btn-ghost btn-sm" onClick={() => loadMore(q)}>
                    retry
                  </button>
                </div>
              )}
              {state.hasNext && !state.loadingMore && !state.moreError && (
                <div class="py-6 flex justify-center">
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm opacity-60"
                    onClick={() => loadMore(q)}
                  >
                    more results
                  </button>
                </div>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
