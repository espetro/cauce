/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/admin?tab=audit` state (FX-05): `GET /api/audit` rows +
 * actor/action facet selects. `audit_facets` has no wire twin — the
 * select options come from an unfiltered `limit=…` scan (documented
 * on #266), so a rare actor outside that window can be missing. Row
 * `request_id` links keep pointing at the HTMX `/trace/{id}` page —
 * no JSON twin exists.
 */

import type { AuditRow } from "../../../types/AuditRow.js";
import { fetchAudit } from "../../lib/api.js";
import { fmtTs } from "../../lib/format.js";
import { spa } from "../../lib/i18n.js";

const A = () => spa.audit;

const LIST_LIMIT = 50;
/** Rows scanned for the facet selects (same bound as the page's cap). */
const FACET_SCAN = 200;

export interface AuditRowView {
  key: string;
  ts: string;
  actor: string;
  action: string;
  target: string;
  requestId: string;
  shortRequestId: string;
  details: string;
}

/** `filtered_empty_message` parity: `actor "x" and action "y"` clauses. */
function filteredEmpty(actor: string, action: string): string {
  const clauses: string[] = [];
  if (actor !== "") clauses.push(`${A().actor_label} "${actor}"`);
  if (action !== "") clauses.push(`${A().action_label} "${action}"`);
  return (
    `${A().filtered_empty_prefix} ` +
    clauses.join(` ${A().filtered_empty_and} `) +
    "."
  );
}

function rowView(r: AuditRow, i: number): AuditRowView {
  const rid = r.request_id ?? "";
  return {
    key: String(r.id ?? rid) + "-" + i,
    ts: fmtTs(r.ts),
    actor: r.actor,
    action: r.action,
    target: r.target,
    requestId: rid,
    shortRequestId: rid.slice(0, 8),
    details:
      r.details == null
        ? ""
        : typeof r.details === "string"
          ? r.details
          : JSON.stringify(r.details, null, 2),
  };
}

export function createAuditTab() {
  const state = $state({
    loading: true,
    error: "",
    actor: "",
    action: "",
    actorOptions: [] as string[],
    actionOptions: [] as string[],
    rows: [] as AuditRowView[],
    filtered: false,
    capped: false,
    countLine: "",
    emptyLine: "",
  });

  async function loadFacets(): Promise<void> {
    try {
      const p = new URLSearchParams({ limit: String(FACET_SCAN) });
      const rows = await fetchAudit(p);
      const actors = new Set<string>();
      const actions = new Set<string>();
      for (const r of rows) {
        actors.add(r.actor);
        actions.add(r.action);
      }
      state.actorOptions = [...actors].sort();
      state.actionOptions = [...actions].sort();
    } catch {
      // Facets are a nicety — leave the selects at `any` if it fails.
    }
  }

  async function refresh(): Promise<void> {
    state.loading = true;
    state.error = "";
    const p = new URLSearchParams({ limit: String(LIST_LIMIT) });
    if (state.actor !== "") p.set("actor", state.actor);
    if (state.action !== "") p.set("action", state.action);
    state.filtered = state.actor !== "" || state.action !== "";
    try {
      const rows = await fetchAudit(p);
      state.rows = rows.map(rowView);
      state.capped = rows.length >= LIST_LIMIT;
      const word = rows.length === 1 ? A().row : A().rows;
      state.countLine =
        `${rows.length} ${word}` + (state.filtered ? ` ${A().matching}` : "");
      state.emptyLine =
        state.rows.length === 0
          ? state.filtered
            ? filteredEmpty(state.actor, state.action)
            : A().empty
          : "";
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  async function run(params: URLSearchParams): Promise<void> {
    state.actor = params.get("actor") ?? "";
    state.action = params.get("action") ?? "";
    await Promise.all([loadFacets(), refresh()]);
  }

  function submit(navigate: (to: string) => void): void {
    const p = new URLSearchParams({ tab: "audit" });
    if (state.actor !== "") p.set("actor", state.actor);
    if (state.action !== "") p.set("action", state.action);
    navigate("/app/admin?" + p.toString());
  }

  return { state, run, refresh, submit, LIST_LIMIT };
}
