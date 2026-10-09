/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * The SPA's capability store (FX-07 §7.4). Bootstraps off the two
 * always-open endpoints — `GET /api/capabilities` (mode, derived role,
 * feature flags) and `GET /api/instance` (name, version, engine ids,
 * `aiEnabled`, `indexOnClick`). Both stay reachable in public mode,
 * unlike `GET /api/config` which is admin-only there — the server is
 * authoritative, this store just mirrors it.
 *
 * `flags` drives rendering only; authz lives server-side. A
 * `capabilities.flags.*` check must never stand in for the 401 the
 * server returns.
 */

import type { CapabilityFlags } from "../../types/CapabilityFlags.js";
import type { InstanceMode } from "../../types/InstanceMode.js";
import type { Role } from "../../types/Role.js";
import { fetchCapabilities, fetchInstance } from "./api.js";

export interface Capabilities {
  /** `local` (single operator, today's default) or `public`. */
  mode: InstanceMode;
  /** Server-derived role for the current credential — never inferred. */
  role: Role;
  /** The §7.4 flag block — see `CapabilityFlags` for per-flag docs. */
  flags: CapabilityFlags;
  /** `[ai].enabled` — gates the Assist card and the AI-mode segment. */
  aiEnabled: boolean;
  /** `[archive].index_on_click` — arms the per-user archive index. */
  indexOnClick: boolean;
  /** Configured engine ids — the client-side `validate_pin` set. */
  engineIds: string[];
  /** `[server].name` — the public dashboard card's headline. */
  instanceName: string;
  /** `cauce --version` — shown on the instance card. */
  version: string;
  /** Enabled engine count — the card's scale hint. */
  engineCount: number;
  /** Both bootstrap fetches settled (true or failed). */
  loaded: boolean;
}

const LOCAL_FLAGS: CapabilityFlags = {
  adminSurface: true,
  serverHistory: true,
  archiving: true,
  sharedStats: true,
  allowUserKeys: false,
  allowUserBaseUrl: false,
};

const CLOSED_FLAGS: CapabilityFlags = {
  adminSurface: false,
  serverHistory: false,
  archiving: false,
  sharedStats: false,
  allowUserKeys: false,
  allowUserBaseUrl: false,
};

export const capabilities = $state<Capabilities>({
  mode: "local",
  role: "admin",
  flags: LOCAL_FLAGS,
  aiEnabled: false,
  indexOnClick: false,
  engineIds: [],
  instanceName: "",
  version: "",
  engineCount: 0,
  loaded: false,
});

let inflight: Promise<void> | null = null;

/**
 * Bootstrap fetch, deduped — `run()` awaits it so the pin check has the
 * real engine set before the stream opens. Fetch fails closed on flags
 * (everything but the open surface hidden) matching an unreachable API.
 */
export function loadCapabilities(): Promise<void> {
  inflight ??= apply();
  return inflight;
}

/** Refetch after the admin token changes — the role may have flipped. */
export function reloadCapabilities(): Promise<void> {
  inflight = null;
  return loadCapabilities();
}

async function apply(): Promise<void> {
  const [caps, inst] = await Promise.all([fetchCapabilities(), fetchInstance()]);
  if (caps) {
    capabilities.mode = caps.mode;
    capabilities.role = caps.role;
    capabilities.flags = caps.flags;
  } else {
    // Unreachable API: fail closed — no admin surface, no server-backed
    // panels. Local history/index still work off web storage.
    capabilities.mode = "public";
    capabilities.role = "user";
    capabilities.flags = CLOSED_FLAGS;
  }
  if (inst) {
    capabilities.aiEnabled = inst.aiEnabled;
    capabilities.indexOnClick = inst.indexOnClick;
    capabilities.engineIds = inst.engineIds;
    capabilities.instanceName = inst.name;
    capabilities.version = inst.version;
    capabilities.engineCount = inst.engineCount;
  }
  capabilities.loaded = true;
}
