import { useEffect, useRef, useState } from "preact/hooks";
import { usePageTitle } from "../components/Header";
import { Layout } from "../components/Layout";
import { truncate } from "../lib/format";

interface ClickRow {
  id: number;
  query_hash: string;
  query: string;
  result_id: string;
  url: string;
  title: string;
  clicked_at: number;
  source: string;
}

interface ClickStats {
  total: number;
  last_24h: number;
  oldest: number | null;
}

/** GET /history is HTML-only on the backend, so the route fetches the
 * page and extracts the embedded JSON payload (rendered into
 * <script type="application/json" id="oxe-history">). */
async function fetchHistory(
  sinceHours: number | null,
  qText: string,
): Promise<{ rows: ClickRow[]; stats: ClickStats }> {
  const p = new URLSearchParams();
  if (sinceHours != null) p.set("since", String(sinceHours));
  if (qText) p.set("q", qText);
  const res = await fetch(`/history?${p.toString()}`);
  if (!res.ok) throw new Error(`history failed: ${res.status}`);
  const html = await res.text();
  const doc = new DOMParser().parseFromString(html, "text/html");
  const data = doc.querySelector('script[type="application/json"]#oxe-history')?.textContent;
  if (data) {
    const parsed = JSON.parse(data) as { rows: ClickRow[]; stats: ClickStats };
    return {
      rows: parsed.rows ?? [],
      stats: parsed.stats ?? { total: 0, last_24h: 0, oldest: null },
    };
  }
  // legacy fallback: scrape the legacy table
  const rows = Array.from(doc.querySelectorAll("table.rows tbody tr")).map((tr) => {
    const td = tr.querySelectorAll("td");
    return {
      id: 0,
      query_hash: (td[1]?.querySelector("a")?.getAttribute("href") ?? "").replace("/row/", ""),
      query: td[1]?.textContent ?? "",
      result_id: "",
      url: td[3]?.querySelector("a")?.getAttribute("href") ?? "",
      title: td[2]?.textContent ?? "",
      clicked_at: Date.parse(td[0]?.textContent ?? "") / 1000 || 0,
      source: td[4]?.textContent ?? "",
    };
  });
  const m = doc.querySelector("section.hero p")?.textContent ?? "";
  const total = Number(m.match(/(\d+) total/)?.[1] ?? 0);
  const last24 = Number(m.match(/(\d+) clicks/)?.[1] ?? 0);
  return { rows, stats: { total, last_24h: last24, oldest: null } };
}

function fmtLocal(epoch: number): string {
  if (!epoch) return "—";
  const d = new Date(epoch * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export default function HistoryRoute() {
  usePageTitle("history");
  const [since, setSince] = useState<number | null>(null);
  const [qText, setQText] = useState("");
  const [rows, setRows] = useState<ClickRow[]>([]);
  const [stats, setStats] = useState<ClickStats>({ total: 0, last_24h: 0, oldest: null });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    const id = ++seq.current;
    setLoading(true);
    fetchHistory(since, qText)
      .then((r) => {
        if (seq.current !== id) return;
        setRows(r.rows);
        setStats(r.stats);
        setError(null);
      })
      .catch((e: Error) => seq.current === id && setError(e.message))
      .finally(() => seq.current === id && setLoading(false));
  }, [since, qText]);

  const filtered = qText
    ? rows.filter((r) => (r.query || "").toLowerCase().includes(qText.toLowerCase()))
    : rows;

  return (
    <Layout class="w-full max-w-[960px] mx-auto px-4 pb-16">
      <h1 class="text-xl font-semibold mt-6 mb-1">Click history</h1>
      <p class="text-[13px] opacity-60 mb-4">
        {stats.last_24h} clicks in last 24h · {stats.total} total ·{" "}
        {stats.oldest ? `${fmtLocal(stats.oldest)} (oldest)` : "—"}
      </p>

      <div class="flex flex-wrap items-center gap-2 mb-4">
        <select
          class="select select-sm w-32"
          value={since ?? ""}
          onChange={(e) => {
            const v = (e.target as HTMLSelectElement).value;
            setSince(v === "" ? null : Number(v));
          }}
          aria-label="time filter"
        >
          <option value="">all time</option>
          <option value="24">last 24h</option>
          <option value="168">last week</option>
          <option value="720">last month</option>
        </select>
        <input
          type="search"
          class="input input-sm w-56"
          placeholder="filter by query text…"
          value={qText}
          onInput={(e) => setQText((e.target as HTMLInputElement).value)}
          aria-label="query filter"
        />
        {(since != null || qText) && (
          <button
            type="button"
            class="btn btn-ghost btn-sm"
            onClick={() => {
              setSince(null);
              setQText("");
            }}
          >
            clear
          </button>
        )}
      </div>

      {loading && (
        <div class="py-10 flex justify-center" aria-busy="true">
          <span class="loading loading-dots loading-md" />
        </div>
      )}
      {!loading && error && <p class="text-error py-6 text-sm">error: {error}</p>}
      {!loading && !error && filtered.length === 0 && (
        <p class="opacity-60 py-8 text-sm">
          no clicks yet — open a result from the{" "}
          <a href="/" class="link link-primary">
            search
          </a>{" "}
          page.
        </p>
      )}

      {!loading && filtered.length > 0 && (
        <div class="overflow-x-auto">
          <table class="table table-sm">
            <thead>
              <tr class="text-[13px] opacity-60">
                <th>clicked</th>
                <th>query</th>
                <th>title</th>
                <th class="hidden md:table-cell">url</th>
                <th>source</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {filtered.map((r) => (
                <tr key={`${r.id}-${r.url}`} class="align-top">
                  <td class="whitespace-nowrap text-[13px]">{fmtLocal(r.clicked_at)}</td>
                  <td class="text-[13px]">
                    <a href={`/row/${r.query_hash}`} class="link link-primary">
                      {r.query || "(no query)"}
                    </a>
                  </td>
                  <td class="text-[13px] max-w-[220px] truncate">{r.title}</td>
                  <td class="hidden md:table-cell text-[13px] opacity-60 max-w-[260px] truncate">
                    <a
                      href={r.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      class="link link-hover"
                    >
                      {truncate(r.url, 80)}
                    </a>
                  </td>
                  <td class="text-[13px] opacity-70">{r.source}</td>
                  <td>
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
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Layout>
  );
}
