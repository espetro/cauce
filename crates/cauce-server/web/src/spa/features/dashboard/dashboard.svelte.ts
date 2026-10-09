/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/dashboard` feature state (FX-05): `GET /api/stats?days=N`
 * shaped into the same view model `src/dashboard.rs` builds — stacked
 * day bars, hit-rate + tier split, latency, clients, outcomes,
 * reliability, top/zero queries, engine eval, engines table, cache
 * block. All math ports the Rust helpers one-for-one (`day_bars`,
 * `pct_str`, `phase_cell`, `fmt_bytes`, `breaker_label`).
 */

import type { DayCount } from "../../../types/DayCount.js";
import type { EngineStatsRow } from "../../../types/EngineStatsRow.js";
import type { PhaseStats } from "../../../types/PhaseStats.js";
import type { StatsSnapshot } from "../../../types/StatsSnapshot.js";
import { fetchStats } from "../../lib/api.js";
import { engineAnchor } from "../../lib/encodeId.js";
import { fmtBytes, fmtTs, pctStr } from "../../lib/format.js";

export const CHART_W = 640;
export const CHART_H = 100;
const CHART_PAD = 8;
/** 14px of headroom under the baseline for the MM-DD ticks. */
export const CHART_H_TICKS = CHART_H + 14;

export interface DayBar {
  x: number;
  w: number;
  cacheY: number;
  cacheH: number;
  netY: number;
  netH: number;
  title: string;
  tick: string;
}

/** `day_bars` — cache hits at the baseline, network stacked on top. */
export function dayBars(perDay: DayCount[]): DayBar[] {
  const n = perDay.length;
  const max = Math.max(0, ...perDay.map((d) => d.searches));
  if (n === 0 || max === 0) return [];
  const usable = CHART_H - CHART_PAD;
  const slot = CHART_W / n;
  const bw = Math.min(slot * 0.6, 28);
  return perDay.map((d, i) => {
    const x = i * slot + (slot - bw) / 2;
    const cacheH = (d.cache_hits / max) * usable;
    const netH = ((d.searches - d.cache_hits) / max) * usable;
    return {
      x: r1(x),
      w: r1(bw),
      cacheY: r1(CHART_H - cacheH),
      cacheH: r1(cacheH),
      netY: r1(CHART_H - cacheH - netH),
      netH: r1(netH),
      title: `${d.day}: ${d.searches} searches, ${d.cache_hits} cached`,
      // `%m-%d` — the wire `day` is already `YYYY-MM-DD`.
      tick: d.day.slice(5),
    };
  });
}

function r1(v: number): number {
  return Math.round(v * 10) / 10;
}

/** `phase_cell`: `med/p80/p95`, `"-"` without samples. */
export function phaseCell(p: PhaseStats, hasSamples: boolean): string {
  if (!hasSamples) return "-";
  return `${p.median_ms}/${p.p80_ms}/${p.p95_ms}`;
}

export function breakerLabel(state: EngineStatsRow["breaker"]): string {
  return state === "half_open" ? "half-open" : state;
}

export interface SplitRow {
  name: string;
  count: number;
  pct: string;
}

export interface EngineRow {
  id: string;
  cardAnchor: string;
  breaker: string;
  reliability: string;
  requests: number;
  total: string;
  http: string;
  parse: string;
}

export function createDashboardPage() {
  const state = $state({
    loading: true,
    error: "",
    days: 7,
    snap: null as StatsSnapshot | null,
  });

  async function run(params: URLSearchParams): Promise<void> {
    state.loading = true;
    state.error = "";
    const d = params.get("days");
    state.days = d === "30" ? 30 : 7;
    try {
      state.snap = await fetchStats(state.days);
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  /** The whole view model `Dashboard::from_snapshot` builds. */
  const view = () => {
    const snap = state.snap;
    if (!snap) return null;
    const searches = snap.searches;
    const clientTotal = snap.by_client.reduce((a, c) => a + c.searches, 0);
    const outcomeTotal = Object.values(snap.outcomes).reduce(
      (a, n) => a + n,
      0,
    );
    const outcomes: SplitRow[] = [];
    for (const name of ["ok", "error", "rejected"]) {
      const n = snap.outcomes[name];
      if (n != null) {
        outcomes.push({ name, count: n, pct: pctStr(n, outcomeTotal) });
      }
    }
    for (const [name, n] of Object.entries(snap.outcomes)) {
      if (!["ok", "error", "rejected"].includes(name)) {
        outcomes.push({ name, count: n, pct: pctStr(n, outcomeTotal) });
      }
    }
    return {
      hasData: searches > 0,
      hitRatePct: Math.round(snap.hit_rate * 100) + "%",
      totalHits: snap.cache_hits,
      totalSearches: searches,
      bars: dayBars(snap.per_day),
      tierRows: snap.hits_by_tier.map((t) => ({
        tier: t.tier,
        hits: t.hits,
        pct: pctStr(t.hits, searches),
      })),
      lat: snap.latency,
      ttfr: snap.ttfr,
      clients: snap.by_client.map((c) => ({
        name: c.client,
        count: c.searches,
        pct: pctStr(c.searches, clientTotal),
      })),
      outcomes,
      topQueries: snap.top_queries,
      zeroQueries: snap.zero_result_queries,
      deadlineHits: snap.deadline_hits,
      deadlineRate: pctStr(snap.deadline_hits, searches),
      staleServed: snap.admission.stale_served,
      admissionRejected: snap.admission.rejected,
      engines: snap.engines.map(
        (e): EngineRow => ({
          id: e.engine,
          cardAnchor: engineAnchor(e.engine),
          breaker: breakerLabel(e.breaker),
          reliability: Math.round(e.reliability_pct) + "%",
          requests: e.requests,
          total: phaseCell(
            { median_ms: e.median_ms, p80_ms: e.p80_ms, p95_ms: e.p95_ms },
            e.requests > 0,
          ),
          http: phaseCell(e.http, e.requests > 0),
          parse: phaseCell(e.parse, e.requests > 0),
        }),
      ),
      eval: snap.engine_eval
        ? {
            meta:
              snap.engine_eval.date +
              " · " +
              (snap.engine_eval.live ? "live" : "replay"),
            rows: snap.engine_eval.engines.map((e) => ({
              engine: e.engine,
              score: `${e.hits}/${e.cases} · ${Math.round(e.domain_hit_at5 * 100)}%`,
            })),
          }
        : null,
      cacheRows: snap.cache_entries + snap.cache_entries_expired,
      cacheUnexpired: snap.cache_entries,
      cacheExpired: snap.cache_entries_expired,
      cacheDb: fmtBytes(snap.cache_db_bytes),
      cacheNewest: snap.cache_newest_at ? fmtTs(snap.cache_newest_at) : "-",
    };
  };

  return { state, run, view };
}
