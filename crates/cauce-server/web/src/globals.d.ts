/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

// Window globals the templates inject (`var S`/`var SA`/`var AS` i18n
// bundles, `var Q`, `var assistContext`) and the htmx runtime the bundle
// installs, so the .ts modules read them without casts. Present only on
// the pages whose templates emit them — every read stays gated on
// definedness.
//
// The string bundles are typed from the generated `i18n/*.json` mirrors
// of the rust-i18n catalog (`gen_i18n` keeps them in lockstep). `var S`
// historically meant two different bundles — `search.*` on `/search` and
// `answer.*` on `/answer` — so `/answer`'s template emits `var SA`
// instead: one global, one shape, and no init-time narrowing sniff. The
// `typeof import()` queries keep this file ambient (no top-level import,
// so nothing below needs `declare global`).

/** The `search.*` catalog bundle `page.html` injects as `var S`. */
type StreamStrings = typeof import("./i18n/search.json").default;
/** The `assist.*` catalog bundle `page.html` injects as `var AS`. */
type AssistStrings = typeof import("./i18n/assist.json").default;
/** The `answer.*` catalog bundle `answer.html` injects as `var SA`. */
type AnswerStrings = typeof import("./i18n/answer.json").default;

interface Window {
  htmx?: Htmx;
  /** `search_bundle` copy for the streaming SERP (`page.html`). */
  S?: StreamStrings;
  /** `answer_bundle` copy for `/answer` (`answer.html`, `var SA`). */
  SA?: AnswerStrings;
  /** `assist_bundle` copy for the assist card (`page.html`). */
  AS?: AssistStrings;
  /** `/answer` query literal (`var Q`). */
  Q?: string;
  /** Serialized top-K rows the assist card answers from. */
  assistContext?: import("./types/AnswerSource").AnswerSource[];
}

interface Element {
  /** The htmx `sse` extension's EventSource, parked on its element. */
  cauceEventSource?: import("./sse").SseSource;
}

/** `cauce:sse` detail — `{name, data}` with `data` the raw JSON frame. */
interface CauceSseDetail {
  name: string;
  data: string;
}

interface HTMLElementEventMap {
  "cauce:sse": CustomEvent<CauceSseDetail>;
}
