/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/** Selection fallback when the clipboard API is unavailable. */
export function selectText(el: Node, doc: Document = document): void {
  const r = doc.createRange();
  r.selectNodeContents(el);
  const s = doc.defaultView?.getSelection();
  if (!s) return;
  s.removeAllRanges();
  s.addRange(r);
}

/** Clipboard write with the selection fallback (engines/trace pattern). */
export function copyNow(
  text: string,
  el: Node,
  nav: Navigator = navigator,
  doc: Document = document,
): void {
  if (nav.clipboard && nav.clipboard.writeText) {
    nav.clipboard.writeText(text);
  } else {
    selectText(el, doc);
  }
}

/** Footer request id on the engines page is click-to-copy. */
export function initRequestIdCopy(doc: Document = document): void {
  const el = doc.getElementById("page-request-id");
  if (!el) return;
  el.addEventListener("click", () =>
    copyNow(el.textContent ?? "", el, navigator, doc),
  );
}

/** `/trace/{id}`: copy button mirrors the traced request id. */
export function initTracedCopy(doc: Document = document): void {
  const btn = doc.getElementById("copy-traced");
  const el = doc.getElementById("traced-id");
  if (!btn || !el) return;
  btn.addEventListener("click", () =>
    copyNow(el.textContent ?? "", el, navigator, doc),
  );
}
