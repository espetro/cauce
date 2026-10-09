/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/settings` state (FX-05): `GET /api/config` → editable form →
 * `PUT /api/config` (urlencoded dotted names, the same field names the
 * HTMX form posts). Documented degradations (#266): env provenance
 * (`set_by …`, disabled inputs), the `engines_pinned` greying, the
 * `/api/models` datalist and `config_path` have no wire twin — fields
 * render editable, the api_key keeps its `<redacted>` display value
 * which `restore_redacted` accepts on save.
 */

import { createForm, reset } from "@formisch/svelte";
import * as v from "valibot";

import type { Config } from "../../../types/Config.js";
import type { ConfigPutResponse } from "../../../types/ConfigPutResponse.js";
import type { EngineEntry } from "../../../types/EngineEntry.js";
import {
  deleteCacheBulk,
  fetchConfig,
  fetchStats,
  putConfig,
} from "../../lib/api.js";
import { fmtHm, humanBytes } from "../../lib/format.js";
import { spa } from "../../lib/i18n.js";

const S = () => spa.settings;

const engineSchema = v.object({
  id: v.string(),
  kind: v.string(),
  enabled: v.boolean(),
  tier: v.string(),
  proxy: v.string(),
});

export interface EngineFormRow extends v.InferInput<typeof engineSchema> {}

// Formisch (v0.4.0 plan: "formisch once per-field errors prove needed"):
// the dotted-name wire fields are all strings, so the schema only earns
// its keep on the numeric keys — a non-digit value now fails per field
// instead of round-tripping into a `PUT /api/config` 4xx.
const numField = () =>
  v.pipe(v.string(), v.regex(/^\d+$/, S().invalid_number));

const settingsSchema = v.object({
  deadlineMs: numField(),
  ttlS: numField(),
  minResults: numField(),
  hedgeFloorMs: numField(),
  hedgeCeilingMs: numField(),
  maxWaitMs: numField(),
  maxConcurrent: numField(),
  retentionDays: numField(),
  aiBaseUrl: v.string(),
  aiApiKey: v.string(),
  aiModel: v.string(),
  aiMaxTurns: numField(),
  aiMaxSearches: numField(),
  aiProviderBudgetS: numField(),
  aiEnabled: v.boolean(),
  engines: v.array(engineSchema),
});

export interface SettingsForm extends v.InferInput<typeof settingsSchema> {}

function formFrom(c: Config): SettingsForm {
  return {
    deadlineMs: String(c.search.deadline_ms),
    ttlS: String(c.search.ttl_s),
    minResults: String(c.search.min_results),
    hedgeFloorMs: String(c.search.hedge_floor_ms),
    hedgeCeilingMs: String(c.search.hedge_ceiling_ms),
    maxWaitMs: String(c.admission.max_wait_ms),
    maxConcurrent: String(c.admission.max_concurrent_per_engine),
    retentionDays: String(c.logs.retention_days),
    aiBaseUrl: c.ai.base_url,
    aiApiKey: c.ai.api_key,
    aiModel: c.ai.model,
    aiMaxTurns: String(c.ai.max_turns),
    aiMaxSearches: String(c.ai.max_searches),
    aiProviderBudgetS: String(c.ai.provider_budget_s),
    aiEnabled: c.ai.enabled,
    engines: c.engines.map(engineRow),
  };
}

function engineRow(e: EngineEntry): EngineFormRow {
  return {
    id: e.id,
    kind: e.kind,
    enabled: e.enabled,
    tier: e.tier == null ? "" : String(e.tier),
    proxy: e.egress?.proxy ?? "",
  };
}

/** The `name="…"` payload the HTMX form posts, built from the form. */
export function formBody(f: SettingsForm): URLSearchParams {
  const p = new URLSearchParams();
  p.set("search.deadline_ms", f.deadlineMs);
  p.set("search.ttl_s", f.ttlS);
  p.set("search.min_results", f.minResults);
  p.set("search.hedge_floor_ms", f.hedgeFloorMs);
  p.set("search.hedge_ceiling_ms", f.hedgeCeilingMs);
  p.set("admission.max_wait_ms", f.maxWaitMs);
  p.set("admission.max_concurrent_per_engine", f.maxConcurrent);
  p.set("logs.retention_days", f.retentionDays);
  p.set("ai.base_url", f.aiBaseUrl);
  p.set("ai.api_key", f.aiApiKey);
  p.set("ai.model", f.aiModel);
  p.set("ai.max_turns", f.aiMaxTurns);
  p.set("ai.max_searches", f.aiMaxSearches);
  p.set("ai.provider_budget_s", f.aiProviderBudgetS);
  p.set("ai.enabled", f.aiEnabled ? "true" : "false");
  for (const e of f.engines) {
    p.set(`engines.${e.id}.enabled`, e.enabled ? "true" : "false");
    p.set(`engines.${e.id}.tier`, e.tier);
    p.set(`engines.${e.id}.egress.proxy`, e.proxy);
  }
  return p;
}

export function createSettingsPage() {
  const form = createForm({ schema: settingsSchema });
  const state = $state({
    loading: true,
    error: "",
    form: null as SettingsForm | null,
    status: "",
    statusError: false,
    cacheLine: "",
    cacheError: "",
  });

  async function cacheLine(): Promise<void> {
    // `"{total} entries · {unexpired} unexpired · {db size} · newest
    // {HH:MM}"` — full parity: `cache_db_bytes` and `cache_newest_at`
    // are on `StatsSnapshot`.
    try {
      const st = await fetchStats(7);
      const total = st.cache_entries + st.cache_entries_expired;
      const newest =
        st.cache_newest_at == null ? spa.common.dash : fmtHm(st.cache_newest_at);
      state.cacheLine = `${total} ${S().entries} · ${st.cache_entries} ${S().unexpired} · ${humanBytes(st.cache_db_bytes)} · ${S().newest} ${newest}`;
    } catch {
      state.cacheLine = "";
    }
  }

  async function refresh(): Promise<void> {
    state.loading = true;
    state.error = "";
    try {
      const c = await fetchConfig();
      if (c == null) {
        state.error = S().not_saved;
        return;
      }
      state.form = formFrom(c);
      // `reset` sets the fresh config as both the visible input and the
      // dirty-tracking baseline.
      reset(form, { initialInput: state.form });
      void cacheLine();
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  async function save(output: SettingsForm): Promise<void> {
    state.status = "";
    state.statusError = false;
    try {
      const res: ConfigPutResponse = await putConfig(formBody(output));
      state.status = res.effective_after_restart
        ? `${S().saved} · ${S().applies_after_restart} ${res.requires_restart.join(", ")}`
        : S().saved;
      state.form = formFrom(res);
      reset(form, { initialInput: state.form });
      void cacheLine();
    } catch (e) {
      state.status = `${S().could_not_save} (${e instanceof Error ? e.message : String(e)})`;
      state.statusError = true;
    }
  }

  async function bulkDelete(scope: "expired" | "all"): Promise<void> {
    const msg =
      scope === "expired" ? S().confirm_expired : S().confirm_all;
    if (!window.confirm(msg)) return;
    state.cacheError = "";
    try {
      await deleteCacheBulk(scope);
      void cacheLine();
    } catch (e) {
      state.cacheError = e instanceof Error ? e.message : String(e);
    }
  }

  return { state, form, refresh, save, bulkDelete };
}
