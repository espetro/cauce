import { useEffect, useRef, useState } from "preact/hooks";
import { usePageTitle } from "../components/Header";
import { apiStats, type ApiStats } from "../lib/api";
import { fmtBytes, fmtTs } from "../lib/format";

const MAX_BAR_DAYS = 30;

function Panel(props: { title: string; wide?: boolean; children: preact.ComponentChildren }) {
  return (
    <section
      class={`border border-base-300 rounded-lg p-4 min-w-0 ${props.wide ? "md:col-span-2" : ""}`}
    >
      <h2 class="text-sm font-medium mb-3">{props.title}</h2>
      {props.children}
    </section>
  );
}

function Empty() {
  return <p class="opacity-50 text-sm">no data yet</p>;
}

/** Token-based inline SVG sparkline: total searches per day. */
function Sparkline({ days }: { days: { day: string; total: number }[] }) {
  const data = days.slice(-MAX_BAR_DAYS);
  const max = Math.max(...data.map((d) => d.total), 1);
  const W = 240;
  const H = 48;
  const step = data.length > 1 ? W / (data.length - 1) : W;
  const pts = data.map(
    (d, i) => `${(i * step).toFixed(1)},${(H - (d.total / max) * (H - 4) - 2).toFixed(1)}`,
  );
  return (
    <svg viewBox={`0 0 ${W} ${H}`} class="w-full h-12" role="img" aria-label="searches per day">
      <polyline
        points={pts.join(" ")}
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        class="text-primary"
      />
    </svg>
  );
}

function Bar({ value, max, label }: { value: number; max: number; label: string }) {
  const pct = max > 0 ? Math.round((value / max) * 100) : 0;
  return (
    <div class="mb-1">
      <div class="flex justify-between text-[13px]">
        <span class="truncate max-w-[70%]">{label}</span>
        <span class="opacity-60">{value}</span>
      </div>
      <progress
        class="progress progress-primary h-1"
        value={pct}
        max={100}
        aria-label={`${label}: ${value}`}
      />
    </div>
  );
}

export default function DashboardRoute() {
  usePageTitle("dashboard");
  const [stats, setStats] = useState<ApiStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    const id = ++seq.current;
    apiStats()
      .then((s) => seq.current === id && (setStats(s), setError(null)))
      .catch((e: Error) => seq.current === id && setError(e.message));
  }, []);

  return (
    <div class="w-full max-w-[960px] mx-auto px-4 pb-16">
      <h1 class="text-xl font-semibold mt-6 mb-1">oxe stats</h1>
      <p class="text-[13px] opacity-60 mb-4">window: last {stats?.days ?? 14} days</p>

      {error && <p class="text-error py-6 text-sm">error: {error}</p>}
      {!stats && !error && (
        <div class="py-10 flex justify-center" aria-busy="true">
          <span class="loading loading-dots loading-md" />
        </div>
      )}

      {stats && (
        <div class="grid gap-4 md:grid-cols-2">
          <Panel title="searches per day">
            {stats.searches_per_day.some((d) => d.total > 0) ? (
              <>
                <Sparkline days={stats.searches_per_day} />
                <p class="text-[13px] opacity-60 mt-1">
                  {stats.searches_per_day.reduce((a, d) => a + d.total, 0)} searches in window
                </p>
              </>
            ) : (
              <Empty />
            )}
          </Panel>

          <Panel title="cache hit rate">
            {stats.hit_rate.rate != null ? (
              <>
                <div class="text-3xl font-bold">{stats.hit_rate.rate}%</div>
                <p class="text-[13px] opacity-60">
                  {stats.hit_rate.cache_hits} of {stats.hit_rate.total} served from cache
                </p>
              </>
            ) : (
              <Empty />
            )}
          </Panel>

          <Panel title="network latency">
            {stats.latency_ms.p50 != null ? (
              <div class="grid grid-cols-3 gap-2 text-center">
                {(["p50", "p90", "p99"] as const).map((p) => (
                  <div key={p}>
                    <div class="text-xl font-semibold">{Math.round(stats.latency_ms[p] ?? 0)}</div>
                    <div class="text-[13px] opacity-60">{p} ms</div>
                  </div>
                ))}
              </div>
            ) : (
              <Empty />
            )}
          </Panel>

          <Panel title="client split">
            {stats.client_split.length > 0 ? (
              <div>
                {stats.client_split.map((c) => (
                  <Bar
                    key={c.client}
                    value={c.count}
                    label={c.client}
                    max={Math.max(...stats.client_split.map((x) => x.count))}
                  />
                ))}
              </div>
            ) : (
              <Empty />
            )}
          </Panel>

          <Panel title="top queries" wide>
            {stats.top_queries.length > 0 ? (
              <div>
                {stats.top_queries.slice(0, 10).map((q) => (
                  <Bar
                    key={q.query}
                    value={q.count}
                    label={q.query}
                    max={Math.max(...stats.top_queries.slice(0, 10).map((x) => x.count))}
                  />
                ))}
              </div>
            ) : (
              <Empty />
            )}
          </Panel>

          <Panel title="zero-result queries" wide>
            {stats.zero_result_queries.length > 0 ? (
              <ul class="text-[13px] space-y-1">
                {stats.zero_result_queries.slice(0, 10).map((q) => (
                  <li key={q.query} class="flex justify-between gap-4">
                    <span class="truncate">{q.query}</span>
                    <span class="opacity-60 whitespace-nowrap">{fmtTs(q.last_seen)}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p class="opacity-50 text-sm">none 🎉</p>
            )}
          </Panel>

          <Panel title="cache" wide>
            <table class="table table-sm text-[13px]">
              <tbody>
                <tr>
                  <td class="opacity-60">rows</td>
                  <td class="text-right">{stats.cache.rows}</td>
                  <td class="opacity-60">unexpired</td>
                  <td class="text-right">{stats.cache.unexpired_rows}</td>
                </tr>
                <tr>
                  <td class="opacity-60">db size</td>
                  <td class="text-right">{fmtBytes(stats.cache.db_size_bytes)}</td>
                  <td class="opacity-60">total hits</td>
                  <td class="text-right">{stats.cache.total_hits}</td>
                </tr>
                <tr>
                  <td class="opacity-60">newest</td>
                  <td class="text-right">{fmtTs(stats.cache.newest)}</td>
                  <td class="opacity-60">oldest</td>
                  <td class="text-right">{fmtTs(stats.cache.oldest_unexpired)}</td>
                </tr>
              </tbody>
            </table>
          </Panel>
        </div>
      )}
    </div>
  );
}
