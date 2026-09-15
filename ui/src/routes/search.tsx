import { useCallback, useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { Header, useAiAvailable, usePageTitle } from "../components/Header";
import { useModels } from "../components/ModeSegments";
import { recordClick } from "../lib/api";
import { ResultCard } from "../features/search/ResultCard";
import {
  cachedAgeOf,
  isCacheHit,
  metaLine,
  pagerLetters,
  useSearch,
} from "../features/search/useSearch";
import { searchUrl } from "../features/search/pager";

const MAX_PAGES = 10;
import { fmtDur } from "../lib/format";
import { AnswerView } from "../features/answer/AnswerView";
import { useAnswer } from "../features/answer/useAnswer";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";
type Mode = "traditional" | "ai";

export default function SearchRoute() {
  const { path, query, route } = useLocation();
  const q = String(query?.q ?? "");
  const page = Math.max(1, Number(query?.p ?? 1) || 1);
  const urlMode = query?.mode === "ai" ? "ai" : "traditional";
  usePageTitle(q || "search");

  const aiAvailable = useAiAvailable();
  const { models, error: modelsError } = useModels();
  const [input, setInput] = useState(q);
  const [mode, setMode] = useState<Mode>(urlMode);
  const { state, run, refresh } = useSearch();
  const answer = useAnswer();

  useEffect(() => {
    localStorage.setItem(MODE_KEY, mode);
  }, [mode]);

  // AI mode is unavailable: ?mode=ai URLs stay on classic results (no redirect)
  // with a small inline notice; the disabled toggle communicates why.
  const aiModeBlocked = mode === "ai" && aiAvailable === false;
  const effectiveMode: Mode = aiModeBlocked ? "traditional" : mode;

  useEffect(() => {
    setInput(q);
    if (q && effectiveMode === "traditional") run(q, page);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, page]);

  // canonical url: ai mode is expressed via &mode=ai; preserve other params
  useEffect(() => {
    if (urlMode !== mode) {
      const extra: Record<string, string> = {};
      if (typeof query?.settings === "string") extra.settings = query.settings;
      route(searchUrl({ q, page, mode, extra }), true);
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

  const goPage = (p: number) => {
    const extra: Record<string, string> = {};
    if (typeof query?.settings === "string") extra.settings = query.settings;
    if (query?.mode === "ai") extra.mode = "ai";
    route(searchUrl({ q, page: p, extra }));
  };

  const { payload, loading, error } = state;
  const results = payload?.results ?? [];
  const qHash = payload?._q_hash ?? "";

  return (
    <div class="min-h-screen flex flex-col">
      <Header path={path} />
      <main class="w-full max-w-[652px] mx-auto px-4 pb-16">
        <div class="pt-4 flex flex-col gap-3">
          <SearchBox
            value={input}
            onInput={setInput}
            onSubmit={submit}
            busy={effectiveMode === "ai" ? !answer.state.done : loading}
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
          {effectiveMode === "traditional" && (results.length > 0 || payload) && (
            <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]">
              <span class="opacity-60">{metaLine(payload) || (loading ? "searching…" : "")}</span>
              {isCacheHit(payload) && (
                <span
                  class="tooltip"
                  data-tip="Actually search the web (refreshes this cache entry)"
                >
                  <button
                    type="button"
                    class="badge badge-sm badge-ghost cursor-pointer"
                    aria-label="cached result: click to refresh from the web"
                    onClick={() => refresh(q)}
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
                          results: payload.results,
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
              <div class="py-10 flex justify-center" aria-busy="true">
                <span class="loading loading-dots loading-md" />
              </div>
            )}

            {!loading && error && (
              <div class="py-10 text-sm">
                <p class="text-error mb-3">error: {error}</p>
                <button type="button" class="btn btn-sm" onClick={() => run(q, page)}>
                  retry
                </button>
                {aiAvailable === true && (
                  <button type="button" class="btn btn-ghost btn-sm ml-2" onClick={() => askAi(q)}>
                    ask AI instead
                  </button>
                )}
              </div>
            )}

            {!loading && !error && payload && results.length === 0 && (
              <div class="py-10 text-sm">
                <p class="opacity-60 mb-3">no results</p>
                {aiAvailable === true && (
                  <button type="button" class="btn btn-sm" onClick={() => askAi(q)}>
                    ask AI instead
                  </button>
                )}
              </div>
            )}

            {!loading && results.length > 0 && (
              <div class="divide-y divide-base-300">
                {results.map((r) => (
                  <ResultCard
                    key={r.id || r.url}
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
                ))}
              </div>
            )}

            {!loading && results.length > 0 && <Pager q={q} page={page} goPage={goPage} />}
          </>
        )}
      </main>
    </div>
  );
}

/** Google-letters-style pager: the word "oxe" where each letter after
 * the first is a page link; current page is darker/bold. Subtle, token
 * styled. Prev/next arrows at the edges. */
export function Pager({
  q,
  page,
  goPage,
}: {
  q: string;
  page: number;
  goPage: (p: number) => void;
}) {
  const total = Math.min(page + 1, MAX_PAGES); // next presence is signal enough
  const letters = pagerLetters(total);
  return (
    <nav class="flex items-center justify-center gap-3 py-6 text-sm" aria-label="pagination">
      {page > 1 && (
        <a
          href={searchUrl({ q, page: page - 1 })}
          onClick={(e) => {
            e.preventDefault();
            goPage(page - 1);
          }}
          rel="prev"
          aria-label="previous page"
          class="opacity-50 hover:opacity-100"
        >
          &larr;
        </a>
      )}
      <span class="flex items-baseline gap-1.5 font-mono">
        {letters.map((letter, i) => {
          const p = i + 1;
          const active = p === page;
          return active ? (
            <span key={p} class="font-semibold opacity-90" aria-current="page">
              {letter}
            </span>
          ) : (
            <a
              key={p}
              href={searchUrl({ q, page: p })}
              onClick={(e) => {
                e.preventDefault();
                goPage(p);
              }}
              class="opacity-40 hover:opacity-90"
              aria-label={`page ${p}`}
            >
              {letter}
            </a>
          );
        })}
      </span>
      {page < MAX_PAGES && (
        <a
          href={searchUrl({ q, page: page + 1 })}
          onClick={(e) => {
            e.preventDefault();
            goPage(page + 1);
          }}
          rel="next"
          aria-label="next page"
          class="opacity-50 hover:opacity-100"
        >
          &rarr;
        </a>
      )}
    </nav>
  );
}
