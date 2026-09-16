import { useEffect, useRef, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { usePageTitle } from "../components/Header";
import { deleteHistory, fetchApiHistory, type HistoryScope } from "../lib/api";
import type { HistoryRow } from "../lib/schemas";
import { truncate } from "../lib/format";

const SINCE_VALUES: HistoryScope[] = ["24", "168", "720", "all"];

function parseSince(v: string | undefined): HistoryScope {
  return SINCE_VALUES.includes(v as HistoryScope) ? (v as HistoryScope) : "all";
}

function fmtLocal(epoch: number): string {
  if (!epoch) return "—";
  const d = new Date(epoch * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** Two-step delete: pick a scope, then confirm. `all` requires a second
 * click on the same button (double-click confirm). */
function DeleteControls({
  onDeleted,
  onError,
}: {
  onDeleted: () => void;
  onError: (m: string) => void;
}) {
  const [scope, setScope] = useState<"24h" | "7d" | "30d" | "all">("24h");
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);

  const run = () => {
    setBusy(true);
    deleteHistory(scope)
      .then(() => {
        setArmed(false);
        onDeleted();
      })
      .catch((e: Error) => onError(e.message))
      .finally(() => setBusy(false));
  };

  return (
    <div class="flex items-center gap-2">
      <select
        class="select select-sm w-40"
        value={scope}
        onChange={(e) => {
          setScope((e.target as HTMLSelectElement).value as typeof scope);
          setArmed(false);
        }}
        aria-label="delete scope"
      >
        <option value="24h">older than 24h</option>
        <option value="7d">older than 7d</option>
        <option value="30d">older than 30d</option>
        <option value="all">all history</option>
      </select>
      <button
        type="button"
        class={`btn btn-sm ${armed ? "btn-error" : "btn-ghost text-error"}`}
        disabled={busy}
        onClick={() => (armed ? run() : setArmed(true))}
        onBlur={() => setArmed(false)}
      >
        {armed ? (scope === "all" ? "really delete all?" : "confirm delete") : "delete…"}
      </button>
    </div>
  );
}

export default function HistoryRoute() {
  usePageTitle("history");
  const { query, route } = useLocation();
  // since/qf are URL-addressable (contract: /history?since=24&qf=python)
  const since = parseSince(query?.since);
  const qf = String(query?.qf ?? "");

  const [items, setItems] = useState<HistoryRow[]>([]);
  const [counts, setCounts] = useState({ clicks: 0, cache_rows: 0 });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  const load = (s: HistoryScope, q: string) => {
    const id = ++seq.current;
    setLoading(true);
    fetchApiHistory(s, q)
      .then((r) => {
        if (seq.current !== id) return;
        setItems(
          r.items.map((it): HistoryRow => ({
            ...it,
            sort_at: it.kind === "click" ? it.clicked_at : it.created_at,
          })),
        );
        setCounts({ clicks: r.clicks, cache_rows: r.cache_rows });
        setError(null);
      })
      .catch((e: Error) => seq.current === id && setError(e.message))
      .finally(() => seq.current === id && setLoading(false));
  };

  useEffect(() => {
    load(since, qf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [since, qf]);

  const setParam = (key: "since" | "qf", value: string) => {
    const sp = new URLSearchParams(window.location.search);
    if (value && !(key === "since" && value === "all")) sp.set(key, value);
    else sp.delete(key);
    const qs = sp.toString();
    route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
  };

  const clearFilters = () => {
    const sp = new URLSearchParams(window.location.search);
    sp.delete("since");
    sp.delete("qf");
    const qs = sp.toString();
    route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
  };

  return (
    <div class="w-full max-w-[960px] mx-auto px-4 pb-16">
      <h1 class="text-xl font-semibold mt-6 mb-1">History</h1>
      <p class="text-[13px] opacity-60 mb-4">
        {counts.clicks} clicks · {counts.cache_rows} cached searches · newest first
      </p>

      <div class="flex flex-wrap items-center gap-2 mb-4">
        <select
          class="select select-sm w-32"
          value={since}
          onChange={(e) => setParam("since", (e.target as HTMLSelectElement).value)}
          aria-label="time filter"
        >
          <option value="all">all time</option>
          <option value="24">last 24h</option>
          <option value="168">last week</option>
          <option value="720">last month</option>
        </select>
        <input
          type="search"
          class="input input-sm w-56"
          placeholder="filter by query text…"
          value={qf}
          onInput={(e) => setParam("qf", (e.target as HTMLInputElement).value)}
          aria-label="query filter"
        />
        {(since !== "all" || qf) && (
          <button type="button" class="btn btn-ghost btn-sm" onClick={clearFilters}>
            clear
          </button>
        )}
        <div class="ml-auto">
          <DeleteControls onDeleted={() => load(since, qf)} onError={setError} />
        </div>
      </div>

      {loading && (
        <div class="py-10 flex justify-center" aria-busy="true">
          <span class="loading loading-dots loading-md" />
        </div>
      )}
      {!loading && error && <p class="text-error py-6 text-sm">error: {error}</p>}
      {!loading && !error && items.length === 0 && (
        <p class="opacity-60 py-8 text-sm">
          nothing here yet — open a result from the{" "}
          <a href="/" class="link link-primary">
            search
          </a>{" "}
          page.
        </p>
      )}

      {!loading && items.length > 0 && (
        <div class="overflow-x-auto">
          <table class="table table-sm">
            <thead>
              <tr class="text-[13px] opacity-60">
                <th>when</th>
                <th>kind</th>
                <th>query</th>
                <th class="hidden md:table-cell">detail</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {items.map((r) => (
                <tr key={`${r.kind}-${r.query_hash}-${r.sort_at}`} class="align-top">
                  <td class="whitespace-nowrap text-[13px]">{fmtLocal(r.sort_at)}</td>
                  <td class="text-[13px]">
                    <span
                      class={`badge badge-sm ${r.kind === "click" ? "badge-primary" : "badge-ghost"}`}
                    >
                      {r.kind === "click" ? `click · ${r.source ?? "web"}` : "search"}
                    </span>
                  </td>
                  <td class="text-[13px]">
                    <a href={`/row/${r.query_hash}`} class="link link-primary">
                      {r.query || "(no query)"}
                    </a>
                  </td>
                  <td class="hidden md:table-cell text-[13px] opacity-60 max-w-[300px] truncate">
                    {r.kind === "click" ? (
                      <a
                        href={r.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="link link-hover"
                      >
                        {truncate(r.url ?? "", 80)}
                      </a>
                    ) : (
                      <>
                        {r.hits ?? 0} hits · expires {fmtLocal(r.expires_at ?? 0)}
                      </>
                    )}
                  </td>
                  <td>
                    {r.kind === "click" && (
                      <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        onClick={() =>
                          fetch(`/search?q=${encodeURIComponent(r.query || "")}`, {
                            headers: { Accept: "application/json" },
                          })
                            .then((res) => res.text())
                            .then((t) => navigator.clipboard?.writeText(t))
                        }
                      >
                        copy json
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
