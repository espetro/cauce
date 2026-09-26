/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * The htmx `sse` extension: elements carrying `sse-connect` get an
 * `EventSource` whose named frames (`results`, `meta`, `error`) are
 * re-dispatched as `cauce:sse` DOM events on the element. `meta`/`error`
 * are terminal — the source closes itself. Factored as a factory so tests
 * can inject a fake `htmx` and `EventSource`.
 */
export function sseExtension(htmx, EventSourceImpl) {
  return {
    getSelectors() {
      return ["[sse-connect]"];
    },
    onEvent(name, event) {
      const element = event.target || (event.detail && event.detail.elt);
      if (!element) return;

      if (name === "htmx:beforeCleanupElement") {
        if (element.cauceEventSource) element.cauceEventSource.close();
        return;
      }
      if (name !== "htmx:afterProcessNode" || !element.hasAttribute("sse-connect")) return;
      if (element.cauceEventSource) return;

      const source = new EventSourceImpl(element.getAttribute("sse-connect"));
      element.cauceEventSource = source;
      ["results", "meta", "error"].forEach((eventName) => {
        source.addEventListener(eventName, (message) => {
          if (!("data" in message)) return;
          htmx.trigger(element, "cauce:sse", {
            name: eventName,
            data: message.data,
          });
          if (eventName === "meta" || eventName === "error") source.close();
        });
      });
    },
  };
}

/**
 * Register the extension on the page's `htmx` (inlined ahead of the bundle).
 * Pages without htmx (dashboard, audit) simply skip it.
 */
export function registerSseExtension(
  htmx = window.htmx,
  EventSourceImpl = window.EventSource,
) {
  if (!htmx || !EventSourceImpl) return;
  htmx.defineExtension("sse", sseExtension(htmx, EventSourceImpl));
}
