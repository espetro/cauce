/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `"{engine} failed"` interpolation shared by the stream pages: server-side
 * copy arrives with `{name}` placeholders that the client fills in.
 */
export function fmt(
  template: string,
  values: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) =>
    values[name] != null ? String(values[name]) : "",
  );
}
