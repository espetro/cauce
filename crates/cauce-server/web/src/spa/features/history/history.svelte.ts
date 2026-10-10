/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/history` feature state (FX-05): the `GET /api/history` feed,
 * folded exactly like `src/html/history.rs` — clicks join the newest
 * search row sharing their `query_hash`; orphans render as
 * `(click only)` rows (the store-side `search_hashes` probe has no wire
 * twin — see the FX-05 contract comment on #266 — so an off-window
 * click shows rather than drops).
 *
 * The `cached · <age>` source cell is approximated from a single
 * `GET /api/cache?limit=200` listing matched on `key === query_hash`
 * (`Store::cache_states` likewise has no wire twin).
 */

import type { CachedSearch } from "../../../types/CachedSearch.js";
import type { HistoryItem } from "../../../types/HistoryItem.js";
import type { SearchLogRow } from "../../../types/SearchLogRow.js";
import type { AnswerLogRow } from "../../../types/AnswerLogRow.js";
import type { ClickRow } from "../../../types/ClickRow.js";
import {
  deleteAnswerRow,
  deleteHistoryRow,
  fetchCacheList,
  fetchHistory,
} from "../../lib/api.js";
import { capabilities, loadCapabilities } from "../../lib/capabilities.svelte.js";
import { historyLoad, historyRemove } from "../../lib/localHistory.js";
import { fmtDay, fmtHm, humanSeconds } from "../../lib/format.js";
import { fmt, spa } from "../../lib/i18n.js";
import { confirm } from "../../ui/confirm.js";

const H = () => spa.history;

export interface ClickLine {
  domain: string;
  url: string;
  title: string;
  position: string;
}

export interface HistRow {
  kind: "search" | "click" | "answer";
  id: number;
  dayHeader: string | null;
  when: string;
  query: string;
  queryChip: string;
  source: string;
  sourceUrl: string;
  cachedLive: boolean;
  engines: string;
  resultCount: string;
  latency: string;
  client: string;
  originChip: boolean;
  clicks: ClickLine[];
  clicksWord: string;
  deleteConfirm: string;
  queryUrl: string;
  rerunUrl: string;
  jsonUrl: string;
  error: string;
  gone: boolean;
}

export function createHistoryPage() {
  const state = $state({
    loading: true,
    error: "",
    since: "all",
    origin: "user",
    q: "",
    cached: false,
    rows: [] as HistRow[],
    capped: false,
    emptyMessage: "",
    originAllUrl: "",
    deleting: false,
  });

  function filterParams(): URLSearchParams {
    const p = new URLSearchParams();
    if (state.since !== "all") p.set("since", state.since);
    if (state.origin !== "all") p.set("origin", state.origin);
    else p.set("origin", "all");
    if (state.q.trim() !== "") p.set("q", state.q.trim());
    if (state.cached) p.set("cached", "true");
    p.set("limit", "200");
    return p;
  }

  function filtersActive(): boolean {
    return (
      state.since !== "all" ||
      state.origin !== "user" ||
      state.q.trim() !== "" ||
      state.cached
    );
  }

  function emptyMessageFor(items: HistoryItem[]): void {
    const q = state.q.trim();
    if (!state.cached && q === "" && state.since === "all") {
      state.emptyMessage =
        state.origin === "user"
          ? H().ef_origin
          : state.origin === "agent"
            ? H().ef_none + "."
            : H().empty;
      state.originAllUrl =
        state.origin === "user" ? "/app/history?origin=all" : "";
      return;
    }
    let msg = q !== "" ? `${H().ef_match} "${q}"` : H().ef_none;
    if (state.since === "24h") msg += " " + H().in_24h;
    else if (state.since === "7d") msg += " " + H().in_7d;
    else if (state.since === "30d") msg += " " + H().in_30d;
    else if (state.since !== "all") msg += " " + fmt(H().in_since, { v: state.since });
    if (state.cached) msg += " " + H().ef_cached;
    if (state.origin === "user") {
      // HTMX probes the unfiltered store here; the SPA can only say the
      // default filter hid everything and offer the unfiltered view.
      state.originAllUrl = "/app/history?origin=all";
    }
    state.emptyMessage = msg + ".";
  }

  function clickLine(c: ClickRow): ClickLine {
    let host = "";
    try {
      host = new URL(c.url).hostname;
    } catch {
      host = "";
    }
    return {
      domain: host,
      url: c.url,
      title: c.title === "" ? c.url : c.title,
      position: String(c.position + 1),
    };
  }

  interface CacheLike {
    key: string;
    query: string;
    created_at: string;
    expires_at: string;
  }

  function sourceCell(
    s: SearchLogRow,
    cache: Map<string, CacheLike>,
    now: number,
  ): { source: string; sourceUrl: string; cachedLive: boolean } {
    const st = cache.get(s.query_hash);
    if (st) {
      if (new Date(st.expires_at).getTime() > now) {
        const age = humanSeconds(
          (now - new Date(st.created_at).getTime()) / 1000,
        );
        return {
          source: `${H().src_cached} · ${age}`,
          sourceUrl:
            "/app/admin?tab=cache&q=" +
            encodeURIComponent(st.query) +
            "#" +
            st.key,
          cachedLive: true,
        };
      }
      return {
        source: `${H().src_cached} · ${H().src_expired}`,
        sourceUrl: "",
        cachedLive: false,
      };
    }
    const tier =
      s.tier != null ? ` · ${H().tier_prefix}${s.tier}` : "";
    return { source: `${H().src_network}${tier}`, sourceUrl: "", cachedLive: false };
  }

  function fold(items: HistoryItem[], cache: Map<string, CacheLike>): HistRow[] {
    const now = Date.now();
    const searchHashes = new Set<string>();
    for (const i of items) if (i.kind === "search") searchHashes.add(i.query_hash);

    const clicksByHash = new Map<string, ClickLine[]>();
    for (const i of items) {
      if (i.kind === "click" && i.query_hash && searchHashes.has(i.query_hash)) {
        const arr = clicksByHash.get(i.query_hash) ?? [];
        arr.push(clickLine(i));
        clicksByHash.set(i.query_hash, arr);
      }
    }

    const attached = new Set<string>();
    let lastDay = "";
    const rows: HistRow[] = [];
    const dash = spa.common.dash;
    const dayHeader = (ts: string): string | null => {
      const day = fmtDay(ts);
      if (day === lastDay) return null;
      lastDay = day;
      return day;
    };
    for (const item of items) {
      if (item.kind === "search") {
        const s = item;
        const query = s.query_raw ?? s.query;
        const enc = encodeURIComponent(query);
        const clicks = attached.has(s.query_hash)
          ? []
          : (clicksByHash.get(s.query_hash) ?? []);
        attached.add(s.query_hash);
        const src = sourceCell(s, cache, now);
        const clicksWord = clicks.length === 1 ? H().click_one : H().clicks_word;
        rows.push({
          kind: "search",
          id: s.id ?? 0,
          dayHeader: dayHeader(s.ts),
          when: fmtHm(s.ts),
          query,
          queryChip: "",
          source: src.source,
          sourceUrl: src.sourceUrl,
          cachedLive: src.cachedLive,
          engines: s.engines.join(", "),
          resultCount: String(s.result_count),
          latency: String(s.latency_ms),
          client: clientLabel(s.client),
          originChip: s.origin === "agent",
          clicks,
          clicksWord,
          deleteConfirm:
            clicks.length === 0
              ? H().delete_confirm
              : `${H().delete_confirm_clicks_pre} ${clicks.length} ${clicksWord}?`,
          queryUrl: "/app/search?q=" + enc,
          rerunUrl: "/app/search?q=" + enc,
          jsonUrl: "/api/search?q=" + enc,
          error: "",
          gone: false,
        });
      } else if (item.kind === "click") {
        const c = item;
        if (c.query_hash && searchHashes.has(c.query_hash)) continue;
        rows.push({
          kind: "click",
          id: c.id ?? 0,
          dayHeader: dayHeader(c.ts),
          when: fmtHm(c.ts),
          query: "",
          queryChip: "",
          source: dash,
          sourceUrl: "",
          cachedLive: false,
          engines: dash,
          resultCount: dash,
          latency: dash,
          client: clientLabel(c.client),
          originChip: false,
          clicks: [clickLine(c)],
          clicksWord: H().clicks_word,
          deleteConfirm: H().delete_confirm,
          queryUrl: "",
          rerunUrl: "",
          jsonUrl: "",
          error: "",
          gone: false,
        });
      } else {
        const a: AnswerLogRow = item;
        const query = a.query_raw ?? a.query;
        rows.push({
          kind: "answer",
          id: a.id ?? 0,
          dayHeader: dayHeader(a.ts),
          when: fmtHm(a.ts),
          query,
          queryChip: H().chip_ai,
          source: a.status,
          sourceUrl: "",
          cachedLive: false,
          engines: a.model,
          resultCount: String(a.sources.length),
          latency: dash,
          client: clientLabel(a.client),
          originChip: false,
          clicks: [],
          clicksWord: H().clicks_word,
          deleteConfirm: H().delete_confirm_answer,
          queryUrl: "/answer/" + (a.id ?? 0),
          rerunUrl: "/answer?q=" + encodeURIComponent(query),
          jsonUrl: "",
          error: "",
          gone: false,
        });
      }
    }
    return rows;
  }

  function clientLabel(client: HistoryItem["client"]): string {
    if (client === "ui") return "ui";
    if (client === "api") return "api";
    if (client === "cli") return "cli";
    return "mcp:" + client.mcp;
  }

  /**
   * FX-07: apply the route's filters to a browser-local item set —
   * `since` windows on `ts`, `q` substrings the query (the API's `q`
   * does the same server-side), origin collapses to `user` rows only
   * (everything local IS user), `cached` has no local meaning and is
   * ignored.
   */
  function localItems(): HistoryItem[] {
    const since = state.since;
    const cutoff =
      since === "all"
        ? 0
        : Date.now() -
          (since === "24h"
            ? 86400000
            : since === "7d"
              ? 604800000
              : since === "30d"
                ? 2592000000
                : 0);
    const q = state.q.trim().toLowerCase();
    return historyLoad().filter((i) => {
      if (cutoff > 0 && new Date(i.ts).getTime() < cutoff) return false;
      if (state.origin === "agent" || state.origin === "cli") return false;
      if (
        q !== "" &&
        !(i.kind === "click"
          ? i.url.toLowerCase().includes(q) || i.title.toLowerCase().includes(q)
          : (i.query_raw ?? i.query).toLowerCase().includes(q))
      ) {
        return false;
      }
      return true;
    });
  }

  async function run(params: URLSearchParams): Promise<void> {
    state.loading = true;
    // FX-07: the serverHistory flag decides local vs server feed —
    // wait for the real caps before reading it (defaults are local).
    await loadCapabilities();
    state.error = "";
    state.since = params.get("since") ?? "all";
    state.origin = params.get("origin") ?? "user";
    state.q = params.get("q") ?? "";
    state.cached = params.get("cached") === "true" || params.get("cached") === "1";
    try {
      if (!capabilities.flags.serverHistory) {
        // FX-07: browser-local feed — same fold, no cache listing to
        // join (the `/api/cache` surface is off anyway).
        const items = localItems();
        state.capped = items.length >= 200;
        state.rows = fold(items, new Map());
        if (state.rows.length === 0) {
          state.originAllUrl = "";
          emptyMessageFor(items);
        }
        return;
      }
      const [items, listing] = await Promise.all([
        fetchHistory(filterParams()),
        fetchCacheList(new URLSearchParams({ limit: "200" })).catch(
          () => null,
        ),
      ]);
      const cache = new Map<string, CacheLike>();
      if (listing) for (const e of listing) cache.set(e.key, e);
      state.capped = items.length >= 200;
      state.rows = fold(items, cache);
      if (state.rows.length === 0) {
        state.originAllUrl = "";
        emptyMessageFor(items);
      }
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  async function remove(row: HistRow): Promise<void> {
    const ok = await confirm({
      title: H().delete,
      description: row.deleteConfirm,
      danger: true,
    });
    if (!ok) return;
    row.error = "";
    try {
      await loadCapabilities();
      if (!capabilities.flags.serverHistory) {
        historyRemove(row.id);
      } else if (row.kind === "search") await deleteHistoryRow(row.id);
      else await deleteAnswerRow(row.id);
      row.gone = true;
    } catch (e) {
      row.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** GET-form submit — routes client-side so the URL stays shareable. */
  function submit(navigate: (to: string) => void): void {
    const p = new URLSearchParams();
    if (state.since !== "all") p.set("since", state.since);
    if (state.origin !== "user") p.set("origin", state.origin);
    if (state.q.trim() !== "") p.set("q", state.q.trim());
    if (state.cached) p.set("cached", "1");
    const qs = p.toString();
    navigate("/app/history" + (qs ? "?" + qs : ""));
  }

  return { state, run, remove, submit, filtersActive };
}
