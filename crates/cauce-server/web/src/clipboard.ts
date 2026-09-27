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

/**
 * `/history` (`data-page="history"`, `data-copied-label`): `.request-id`
 * click-to-copies; `a.copy-json` fetches the cached `/api/search` payload
 * into the clipboard and flashes the "copied" label, falling back to a
 * selectable `<code>` with the URL on failure.
 */
export function initHistoryCopy(doc: Document = document): void {
  if (!doc.body || doc.body.dataset.page !== "history") return;
  const copiedLabel = doc.body.dataset.copiedLabel || "";
  doc.addEventListener("click", async (e) => {
    const target = e.target;
    const rid =
      target instanceof Element ? target.closest(".request-id") : null;
    if (rid) {
      try {
        await navigator.clipboard.writeText((rid.textContent ?? "").trim());
      } catch {
        selectText(rid, doc);
      }
      return;
    }
    const a =
      target instanceof Element
        ? target.closest<HTMLAnchorElement>("a.copy-json")
        : null;
    if (!a) return;
    e.preventDefault();
    const code = a.closest("td")?.querySelector(".copy-fallback") ?? null;
    try {
      const resp = await fetch(a.getAttribute("href") ?? a.href);
      if (!resp.ok) throw new Error("status " + resp.status);
      await navigator.clipboard.writeText(await resp.text());
      const label = a.dataset.label || a.textContent || "";
      a.dataset.label = label;
      a.textContent = copiedLabel;
      setTimeout(() => {
        a.textContent = label;
      }, 1500);
    } catch {
      if (code instanceof HTMLElement) {
        code.hidden = false;
        code.textContent = a.href;
      }
    }
  });
}

/** `/cache` (`cache-page` body class): a `#id` hash opens that <details>. */
export function initHashDetails(win: Window = window, doc: Document = document): void {
  if (!doc.body || !doc.body.classList.contains("cache-page")) return;
  win.addEventListener("load", () => {
    if (win.location.hash.length > 1) {
      const d = doc.getElementById(win.location.hash.slice(1));
      if (d instanceof HTMLDetailsElement) d.open = true;
    }
  });
}
