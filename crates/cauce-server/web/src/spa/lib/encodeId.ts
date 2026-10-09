/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `encode_id` from `src/settings.rs`, ported: `-` -> `--`, `.` -> `-d`,
 * any other byte outside `[A-Za-z0-9_]` -> `-xHH`. Injective and
 * dot-free, so the anchors the dashboard links to (`engine-da-db`) are
 * the same ids the admin cards render.
 */

const encoder = new TextEncoder();

export function encodeId(path: string): string {
  let out = "";
  for (const b of encoder.encode(path)) {
    if (b === 0x2d) {
      out += "--";
    } else if (b === 0x2e) {
      out += "-d";
    } else if (
      (b >= 0x30 && b <= 0x39) ||
      (b >= 0x41 && b <= 0x5a) ||
      (b >= 0x61 && b <= 0x7a) ||
      b === 0x5f
    ) {
      out += String.fromCharCode(b);
    } else {
      out += "-x" + b.toString(16).padStart(2, "0");
    }
  }
  return out;
}

/**
 * `<id>` -> the engine card's element id — `encode_id("engine.<id>")`,
 * the same input the dashboard's `card_anchor` and `/engines`' `sel`
 * both encode (note the path includes the literal `engine.` prefix:
 * `a.b` -> `engine-da-db`).
 */
export function engineAnchor(id: string): string {
  return encodeId("engine." + id);
}
