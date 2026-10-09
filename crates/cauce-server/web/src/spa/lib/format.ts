/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/** Host line + favicon helpers shared by result rows and source chips. */

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

/** The DDG icon service the SSR rows use (`/ip3/<host>.ico`). */
export function faviconUrl(host: string): string {
  return "https://icons.duckduckgo.com/ip3/" + encodeURIComponent(host) + ".ico";
}

