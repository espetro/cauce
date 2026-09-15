import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { Header, ModeToggle, usePageTitle } from "../components/Header";
import { recordClick } from "../lib/api";
import { ResultCard } from "../features/search/ResultCard";
import { metaLine, useSearch } from "../features/search/useSearch";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";
type Mode = "traditional" | "ai";

export default function SearchRoute() {
  const { path, query, route } = useLocation();
  const q = String(query?.q ?? "");
  const page = Math.max(1, Number(query?.p ?? 1) || 1);
  usePageTitle(q || "search");

  const [input, setInput] = useState(q);
  const [mode, setMode] = useState<Mode>(() =>
    localStorage.getItem(MODE_KEY) === "ai" ? "ai" : "traditional",
  );
  const { state, run } = useSearch();

  useEffect(() => {
    localStorage.setItem(MODE_KEY, mode);
  }, [mode]);

  useEffect(() => {
    setInput(q);
    if (q) run(q, page);
  }, [q, page]);

  useEffect(() => {
    if (q && mode === "ai") {
      // AI surface ships later; keep the url canonical until then.
      route(`/search?q=${encodeURIComponent(q)}`, true);
    }
  }, [q, mode]);

  const submit = (query: string) => {
    const t = query.trim();
    if (!t) return;
    route(`/search?q=${encodeURIComponent(t)}`);
  };

  const goPage = (p: number) => route(`/search?q=${encodeURIComponent(q)}&p=${p}`);

  const { payload, loading, error } = state;
  const results = payload?.results ?? [];
  const qHash = payload?._q_hash ?? "";

  return (
    <div class="min-h-screen flex flex-col">
      <Header path={path} />
      <main class="w-full max-w-[652px] mx-auto px-4 pb-16">
        <div class="pt-4 flex flex-col gap-3">
          <div class="flex flex-col sm:flex-row sm:items-center gap-3">
            <SearchBox
              value={input}
              onInput={setInput}
              onSubmit={submit}
              busy={loading}
              size="md"
            />
            <ModeToggle mode={mode} onChange={(m) => setMode(m)} size="xs" />
          </div>
          {(results.length > 0 || payload) && (
            <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px]">
              <span class="opacity-60">
                {metaLine(payload, null, null) || (loading ? "searching…" : "")}
              </span>
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
          </div>
        )}

        {!loading && !error && payload && results.length === 0 && (
          <div class="py-10 text-sm">
            <p class="opacity-60 mb-3">no results</p>
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

        {!loading && results.length >= 10 && (
          <nav class="flex items-center justify-center gap-4 py-6 text-sm" aria-label="pagination">
            {page > 1 && (
              <a
                href={`/search?q=${encodeURIComponent(q)}&p=${page - 1}`}
                onClick={(e) => {
                  e.preventDefault();
                  goPage(page - 1);
                }}
                rel="prev"
              >
                &lt; previous
              </a>
            )}
            <span class="opacity-60">page {page}</span>
            <a
              href={`/search?q=${encodeURIComponent(q)}&p=${page + 1}`}
              onClick={(e) => {
                e.preventDefault();
                goPage(page + 1);
              }}
              rel="next"
            >
              next &gt;
            </a>
          </nav>
        )}
      </main>
    </div>
  );
}
