/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/admin?tab=engines` state (FX-05): `GET /api/engines` cards +
 * the audited `reset`/`enable`/`disable` posts + the inline test that
 * used to fetch the `search_fragment.html` arm (the SPA calls the
 * JSON arm and renders `test_results` itself — same meta line, same
 * rows). Degradations noted on #266: `tracked` and the `CAUCE_ENGINES`
 * pin have no wire twin, so `reset` renders for every card (an
 * untracked one answers its 404 inline) and the toggle is never
 * pinned-disabled.
 */

import type { EngineReport } from "../../../types/EngineReport.js";
import type { EngineView } from "../../../types/EngineView.js";
import type { SearchResponse } from "../../../types/SearchResponse.js";
import { fetchEngines, fetchSearch, postEngine } from "../../lib/api.js";
import { encodeId } from "../../lib/encodeId.js";
import { fmtHm, humanSeconds } from "../../lib/format.js";
import { fmt, spa } from "../../lib/i18n.js";

const E = () => spa.engines;
const dash = () => spa.common.dash;

export interface TestState {
  q: string;
  running: boolean;
  metaLine: string;
  error: string;
  results: SearchResponse["results"];
}

export interface EngineCard {
  sel: string;
  testSel: string;
  id: string;
  kindTier: string;
  configured: boolean;
  live: boolean;
  enabled: boolean;
  enabledLabel: string;
  breaker: string;
  breakerClass: string;
  breakerNote: string;
  ewma: string;
  lastOk: string;
  lastError: string;
  p95: string;
  reliability: string;
  requestsToday: number;
  toggleLabel: string;
  notice: string;
  busy: boolean;
  test: TestState;
}

function thousands(n: number): string {
  const s = String(Math.round(n));
  let out = "";
  for (let i = 0; i < s.length; i++) {
    if (i > 0 && (s.length - i) % 3 === 0) out += " ";
    out += s[i];
  }
  return out;
}

function statMs(ms: number): string {
  return ms > 0 ? thousands(ms) + " ms" : dash();
}

function breakerFields(v: EngineView): {
  label: string;
  cls: string;
  note: string;
} {
  if (v.breaker === "closed") {
    return { label: E().breaker_closed, cls: "closed", note: "" };
  }
  if (v.breaker === "half_open") {
    return {
      label: E().breaker_half_open,
      cls: "half-open",
      note: E().breaker_probing,
    };
  }
  let note = "";
  if (v.breaker_until) {
    const left = new Date(v.breaker_until).getTime() - Date.now();
    note =
      left <= 0
        ? E().breaker_elapsed
        : fmt(E().breaker_retries, { rel: humanSeconds(left / 1000) });
  }
  return { label: E().breaker_open, cls: "open", note };
}

function card(v: EngineView): EngineCard {
  const kindTier =
    v.kind === "-" && v.tier == null
      ? dash()
      : v.tier != null
        ? `${v.kind} · t${v.tier}`
        : v.kind;
  const br = breakerFields(v);
  return {
    sel: encodeId("engine." + v.engine),
    testSel: encodeId("test." + v.engine),
    id: v.engine,
    kindTier,
    configured: v.configured,
    live: v.live,
    enabled: v.enabled,
    enabledLabel: v.enabled ? E().enabled_yes : E().enabled_no,
    breaker: br.label,
    breakerClass: br.cls,
    breakerNote: br.note,
    ewma: statMs(v.ewma_ms),
    lastOk: v.last_ok_at
      ? fmt(E().last_ok_fmt, {
          hhmm: fmtHm(v.last_ok_at),
          rel: humanSeconds(
            Math.max(0, Date.now() - new Date(v.last_ok_at).getTime()) / 1000,
          ),
        })
      : dash(),
    lastError: v.last_error ? v.last_error.slice(0, 160) : dash(),
    p95:
      v.p95_ms == null
        ? dash()
        : v.p95_ms === 0
          ? E().sub_ms
          : thousands(v.p95_ms) + " ms",
    reliability:
      v.reliability_pct == null ? dash() : (v.reliability_pct / 100).toFixed(2),
    requestsToday: v.requests_today,
    toggleLabel: v.enabled ? E().action_disable : E().action_enable,
    notice: "",
    busy: false,
    test: { q: "", running: false, metaLine: "", error: "", results: [] },
  };
}

function failureLabel(report: EngineReport | undefined): string {
  if (!report || report.status === "ok") return "";
  const err = report.status.failed;
  if (err === "blocked") return E().test_blocked;
  if (err === "timeout") return E().test_timeout;
  if (err === "no_results") return E().test_no_results;
  if (err === "rate_limited") return E().test_rate_limited;
  if (typeof err === "object" && "parse" in err) return E().test_parse;
  if (typeof err === "object" && "transport" in err) return E().test_transport;
  return E().test_upstream;
}

export function createEnginesTab() {
  const state = $state({
    loading: true,
    error: "",
    cards: [] as EngineCard[],
    summaryLine: "",
  });

  async function refresh(): Promise<void> {
    state.loading = true;
    state.error = "";
    try {
      const views = await fetchEngines();
      state.cards = views.map(card);
      const configured = views.filter((v) => v.configured).length;
      const enabled = views.filter((v) => v.enabled).length;
      const open = views.filter((v) => v.breaker === "open").length;
      state.summaryLine =
        fmt(E().summary, { configured, enabled }) +
        (open > 0 ? fmt(E().summary_open, { open }) : "");
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  async function act(
    c: EngineCard,
    op: "reset" | "enable" | "disable",
  ): Promise<void> {
    c.busy = true;
    c.notice = "";
    try {
      const ack = await postEngine(c.id, op);
      if (op === "reset") {
        await refresh();
      } else {
        c.enabled = ack.enabled;
        c.enabledLabel = ack.enabled ? E().enabled_yes : E().enabled_no;
        c.toggleLabel = ack.enabled ? E().action_disable : E().action_enable;
        c.notice = ack.effective_after_restart
          ? E().toggle_saved_restart
          : E().toggle_saved;
      }
    } catch (e) {
      c.notice = e instanceof Error ? e.message : String(e);
    } finally {
      c.busy = false;
    }
  }

  async function runTest(c: EngineCard): Promise<void> {
    c.test.running = true;
    c.test.error = "";
    c.test.metaLine = "";
    c.test.results = [];
    try {
      const p = new URLSearchParams({
        q: c.test.q || E().test_default,
        engines: c.id,
      });
      const res = await fetchSearch(p);
      c.test.metaLine = fmt(E().test_results, {
        n: res.results.length,
        ms: res.meta.elapsed_ms,
      });
      const failed = res.meta.engines_used.find((r) => r.engine === c.id);
      const label = failureLabel(failed);
      if (label !== "") c.test.error = label;
      c.test.results = res.results;
    } catch (e) {
      c.test.error = e instanceof Error ? e.message : String(e);
    } finally {
      c.test.running = false;
    }
  }

  return { state, refresh, act, runTest };
}
