/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
(function () {
  htmx.defineExtension("sse", {
    getSelectors: function () {
      return ["[sse-connect]"];
    },
    onEvent: function (name, event) {
      var element = event.target || (event.detail && event.detail.elt);
      if (!element) return;

      if (name === "htmx:beforeCleanupElement") {
        if (element.cauceEventSource) element.cauceEventSource.close();
        return;
      }
      if (name !== "htmx:afterProcessNode" || !element.hasAttribute("sse-connect")) return;
      if (element.cauceEventSource) return;

      var source = new EventSource(element.getAttribute("sse-connect"));
      element.cauceEventSource = source;
      ["results", "meta", "error"].forEach(function (eventName) {
        source.addEventListener(eventName, function (message) {
          if (!("data" in message)) return;
          htmx.trigger(element, "cauce:sse", {
            name: eventName,
            data: message.data,
          });
          if (eventName === "meta" || eventName === "error") source.close();
        });
      });
    },
  });
})();
