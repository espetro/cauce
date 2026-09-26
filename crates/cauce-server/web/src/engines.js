/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/engines`: error statuses raised outside the handlers (host guard,
 * transport) produce no swap; show the status in the card's result slot
 * instead of failing silently. The test form names the failed action;
 * reset/toggle get the neutral wording. The two strings arrive as
 * `data-i18n-*` attributes on `<body>` so they stay in `strings.rs`.
 */
export function initEngineErrors(doc = document) {
  const body = doc.body;
  if (!body || body.dataset.i18nTestFailed === undefined) return;
  const testFailed = body.dataset.i18nTestFailed;
  const requestFailed = body.dataset.i18nRequestFailed || "";
  body.addEventListener("htmx:responseError", (ev) => {
    const card = ev.detail.elt.closest(".engine-card");
    const slot = card && card.querySelector(".test-results");
    if (slot) {
      const tpl = ev.detail.elt.tagName === "FORM" ? testFailed : requestFailed;
      slot.textContent = tpl.replace("{status}", ev.detail.xhr.status);
    }
  });
}
