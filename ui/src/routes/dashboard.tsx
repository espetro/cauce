import { useEffect, useRef, useState } from "preact/hooks";
import { usePageTitle } from "../components/Header";
import { Layout } from "../components/Layout";
import { cacheStats } from "../lib/api";
import { fmtBytes, fmtTs } from "../lib/format";

/** GET /history is HTML-only; the search_log dataset is exposed the same
 * way (embedded JSON payload when the renderer provides it). Until the
 * backend ships a stats JSON endpoint, the dashboard renders live cache
 * stats plus a "no search log data" note for the log-derived panels. */
export default function DashboardRoute() {
  usePageTitle("dashboard");
  const [stats, setStats] = useState<Awaited<ReturnType<typeof cacheStats>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    const id = ++seq.current;
    cacheStats()
      .then((s) => seq.current === id && setStats(s))
      .catch((e: Error) => seq.current === id && setError(e.message));
  }, []);

  const panels = [
    { title: "searches per day", body: <p class="opacity-50 text-sm">no search log data yet</p> },
    {
      title: "cache hit rate",
      body: stats ? (
        <div>
          <div class="text-3xl font-bold">
            {stats.total_hits
              ? Math.round((100 * stats.unexpired_rows) / Math.max(stats.rows, 1))
              : 0}
            %
          </div>
          <p class="text-[13px] opacity-60">{stats.total_hits} total hits</p>
        </div>
      ) : (
        <p class="opacity-50 text-sm">no data yet</p>
      ),
    },
    { title: "network latency", body: <p class="opacity-50 text-sm">no data yet</p> },
    { title: "client split", body: <p class="opacity-50 text-sm">no data yet</p> },
    {
      title: "cache",
      body: stats ? (
        <table class="table table-sm text-[13px]">
          <tbody>
            <tr>
              <td class="opacity-60">rows</td>
              <td class="text-right">{stats.rows}</td>
            </tr>
            <tr>
              <td class="opacity-60">unexpired</td>
              <td class="text-right">{stats.unexpired_rows}</td>
            </tr>
            <tr>
              <td class="opacity-60">db size</td>
              <td class="text-right">{fmtBytes(stats.db_size_bytes)}</td>
            </tr>
            <tr>
              <td class="opacity-60">newest</td>
              <td class="text-right">{fmtTs(stats.newest)}</td>
            </tr>
          </tbody>
        </table>
      ) : (
        <p class="opacity-50 text-sm">{error ?? "no data yet"}</p>
      ),
    },
  ];

  return (
    <Layout class="w-full max-w-[960px] mx-auto px-4 pb-16">
      <h1 class="text-xl font-semibold mt-6 mb-1">oxe stats</h1>
      <p class="text-[13px] opacity-60 mb-4">window: last 30 days</p>
      <div class="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(20rem,1fr))]">
        {panels.map((p) => (
          <section key={p.title} class="border border-base-300 rounded-lg p-4 min-w-0">
            <h2 class="text-sm font-medium mb-3">{p.title}</h2>
            {p.body}
          </section>
        ))}
      </div>
    </Layout>
  );
}
