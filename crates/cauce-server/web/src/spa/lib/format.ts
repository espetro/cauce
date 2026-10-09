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

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** `%Y-%m-%d %H:%M` in local time — the table-cell format the pages use. */
export function fmtTs(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return (
    d.getFullYear() +
    "-" +
    pad2(d.getMonth() + 1) +
    "-" +
    pad2(d.getDate()) +
    " " +
    pad2(d.getHours()) +
    ":" +
    pad2(d.getMinutes())
  );
}

/** `%H:%M` local — the history row's time column. */
export function fmtHm(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return pad2(d.getHours()) + ":" + pad2(d.getMinutes());
}

/** `%Y-%m-%d` local — the history day-header grouping key. */
export function fmtDay(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts.slice(0, 10);
  return (
    d.getFullYear() + "-" + pad2(d.getMonth() + 1) + "-" + pad2(d.getDate())
  );
}

/** `human_seconds` from the page modules: `42s`/`5m`/`3h`/`2d`. */
export function humanSeconds(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return s + "s";
  if (s < 3600) return Math.floor(s / 60) + "m";
  if (s < 86400) return Math.floor(s / 3600) + "h";
  return Math.floor(s / 86400) + "d";
}

/** `human_bytes` from the page modules: `842 B`/`4.1 KB`/`1.2 MB`. */
export function humanBytes(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

/** `fmt_bytes` from `src/dashboard.rs`: `B`/`KiB`/`MiB`/`GiB`. */
export function fmtBytes(n: number): string {
  const KIB = 1024;
  const MIB = KIB * 1024;
  const GIB = MIB * 1024;
  if (n >= GIB) return (n / GIB).toFixed(1) + " GiB";
  if (n >= MIB) return (n / MIB).toFixed(1) + " MiB";
  if (n >= KIB) return (n / KIB).toFixed(1) + " KiB";
  return n + " B";
}

/** `pct_str` from `src/dashboard.rs`: `"{:.0}%"` (`0%` on zero total). */
export function pctStr(part: number, total: number): string {
  if (total === 0) return "0%";
  return Math.round((part / total) * 100) + "%";
}

