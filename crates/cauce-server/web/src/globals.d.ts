/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

// Window globals the templates inject (`var S`/`var AS` i18n bundles,
// `var Q`, `var assistContext`) and the htmx runtime the bundle installs,
// so the .ts modules read them without casts. Present only on the pages
// whose templates emit them — every read stays gated on definedness.

interface Window {
  htmx?: Htmx;
  /** `crate::strings::search` copy bundle for the streaming SERP. */
  S?: Record<string, string>;
  /** `crate::strings::assist` copy bundle for the assist card. */
  AS?: Record<string, string>;
  /** `/answer` query literal (`var Q`). */
  Q?: string;
  /** Serialized top-K rows the assist card answers from. */
  assistContext?: unknown[];
}

interface Element {
  /** The htmx `sse` extension's EventSource, parked on its element. */
  cauceEventSource?: EventSource;
}

/** `cauce:sse` detail — `{name, data}` with `data` the raw JSON frame. */
interface CauceSseDetail {
  name: string;
  data: string;
}

interface HTMLElementEventMap {
  "cauce:sse": CustomEvent<CauceSseDetail>;
}
