/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * PUB-03 BYOK: the caller's own AI provider credentials. They live in
 * `localStorage` under a namespaced key and ride `POST /api/answer` as
 * the `ai` override object — never sent anywhere else (the instance is
 * the only endpoint they leave the browser for). Which fields the
 * server honours is gated by `[ai].allow_user_keys` /
 * `[ai].allow_user_base_url` — read off `capabilities.flags`.
 */

import type { AiOverride } from "../../types/AiOverride.js";
import type { AiProtocol } from "../../types/AiProtocol.js";
import type { CapabilityFlags } from "../../types/CapabilityFlags.js";

const KEY = "cauce:byok";

export interface ByokCreds {
  api_key: string;
  model: string;
  /** `""` keeps the instance's protocol. */
  protocol: string;
  base_url: string;
}

const EMPTY: ByokCreds = { api_key: "", model: "", protocol: "", base_url: "" };

function read(): ByokCreds {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw != null) return { ...EMPTY, ...(JSON.parse(raw) as Partial<ByokCreds>) };
  } catch {
    /* malformed or unavailable storage falls back to empty */
  }
  return { ...EMPTY };
}

export const byok = $state<ByokCreds>(read());

/** Save the whole credential set (an empty api_key clears it). */
export function setByok(next: ByokCreds): void {
  Object.assign(byok, {
    api_key: next.api_key.trim(),
    model: next.model.trim(),
    protocol: next.protocol,
    base_url: next.base_url.trim(),
  });
  try {
    if (byok.api_key === "") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, JSON.stringify(byok));
  } catch {
    /* private-mode storage failures leave the in-memory copy */
  }
}

/**
 * The `ai` body field for `POST /api/answer` — `null` when no key is
 * set or the instance doesn't honour user keys at all, so an unattached
 * request bills the instance's `[ai]` (and its `free_daily_answers`
 * budget) exactly as before. Only fields the server advertises ride
 * the wire: a stored `base_url` left over from before the operator
 * closed `allow_user_base_url` is dropped here (the disabled input
 * can't clear it), not turned into a 403 on every answer.
 */
export function byokWire(flags: CapabilityFlags): AiOverride | null {
  if (byok.api_key === "" || !flags.allowUserKeys) return null;
  return {
    api_key: byok.api_key,
    model: byok.model === "" ? null : byok.model,
    protocol: byok.protocol === "" ? null : (byok.protocol as AiProtocol),
    base_url:
      flags.allowUserBaseUrl && byok.base_url !== "" ? byok.base_url : null,
  };
}
