/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Feature gates the SSR templates get from `AppState` (`answer_available`,
 * `index_on_click`). The SPA reads the redacted `GET /api/config` view
 * instead — `ai.enabled` + `archive.index_on_click` are the same flags
 * the templates branch on. Fetch fails open to "feature off", matching
 * a server without the answer loop.
 */

import { fetchConfig } from "./api.js";

export interface Capabilities {
  /** `[ai].enabled` — gates the Assist card and the AI-mode segment. */
  aiEnabled: boolean;
  /** `[archive].index_on_click` — arms the `/api/pages` beacon. */
  indexOnClick: boolean;
  /** Configured engine ids — the client-side `validate_pin` set. */
  engineIds: string[];
  /** The `/api/config` fetch settled (true or failed). */
  loaded: boolean;
}

export const capabilities = $state<Capabilities>({
  aiEnabled: false,
  indexOnClick: false,
  engineIds: [],
  loaded: false,
});

let inflight: Promise<void> | null = null;

/**
 * `/api/config` once, deduped — `run()` awaits it so the pin check has
 * the real engine set before the stream opens (the SSR page validates
 * the pin before it even renders the shell).
 */
export function loadCapabilities(): Promise<void> {
  inflight ??= applyConfig();
  return inflight;
}

async function applyConfig(): Promise<void> {
  const cfg = await fetchConfig();
  if (cfg) {
    capabilities.aiEnabled = cfg.ai.enabled;
    capabilities.indexOnClick = cfg.archive.index_on_click;
    capabilities.engineIds = cfg.engines.map((e) => e.id);
  }
  capabilities.loaded = true;
}
