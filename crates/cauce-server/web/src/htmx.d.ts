/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

// Ambient htmx 2.x surface — the slice the bundle actually uses. htmx.org
// ships `dist/htmx.esm.d.ts` for the full API; the pages talk to htmx
// through `window.htmx` (see globals.d.ts) and `htmx:*` DOM events.

/** The api object `init` receives (json-enc reads expression vars off it). */
interface HtmxExtensionApi {
  getExpressionVars?(elt: Element): Record<string, unknown>;
}

/** Extension shape `htmx.defineExtension` accepts (`sse`, `json-enc`). */
interface HtmxExtension {
  init?(api: HtmxExtensionApi): void;
  getSelectors?(): string[] | null;
  onEvent?(name: string, event: Event | CustomEvent): boolean | void;
  encodeParameters?(
    xhr: XMLHttpRequest,
    parameters: FormData,
    elt: Element,
  ): string | null;
}

interface Htmx {
  defineExtension(name: string, extension: HtmxExtension): void;
  trigger(elt: Element, name: string, detail?: unknown): void;
  ajax(verb: string, url: string, context?: unknown): void;
  process(elt: Element): void;
  find(selector: string): Element | null;
}

/** `htmx:responseError` detail — the issuing element and its xhr. */
interface HtmxResponseErrorDetail {
  elt: Element;
  xhr: XMLHttpRequest;
}

interface HTMLElementEventMap {
  "htmx:responseError": CustomEvent<HtmxResponseErrorDetail>;
  "htmx:afterProcessNode": CustomEvent<{ elt: Element }>;
  "htmx:beforeCleanupElement": CustomEvent<{ elt: Element }>;
}
