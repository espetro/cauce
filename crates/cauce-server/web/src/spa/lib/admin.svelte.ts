/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * FX-07: the instance admin token. On a `public` instance the operator
 * authenticates with `Authorization: Bearer <token>`; the token lives in
 * `localStorage` under a namespaced key and is attached to every `/api/*`
 * call by `api.ts`. It never leaves the browser except to the instance
 * itself.
 */

const KEY = "cauce:admin-token";

function read(): string {
  try {
    return localStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

export const adminToken = $state({ value: read() });

/** Save (or clear, on empty) the token the operator pasted. */
export function setAdminToken(raw: string): void {
  const value = raw.trim();
  adminToken.value = value;
  try {
    if (value === "") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, value);
  } catch {
    /* private-mode storage failures leave the in-memory copy */
  }
}

/** The `Authorization` header fragment `api.ts` spreads into each call. */
export function authHeader(): Record<string, string> {
  return adminToken.value === ""
    ? {}
    : { Authorization: "Bearer " + adminToken.value };
}
