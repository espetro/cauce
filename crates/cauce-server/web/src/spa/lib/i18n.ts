/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Generated string bundles (`gen_i18n` writes them from the rust-i18n
 * catalog; `mise run web` regenerates + diffs them). `spa` is the nested
 * chrome/form bundle; `S`/`AS`/`SA` mirror the inline `var S`/`var AS`/
 * `var SA` literals the streaming/assist/answer pages used to ship.
 */

import spaBundle from "../../i18n/spa.json";
import searchBundle from "../../i18n/search.json";
import assistBundle from "../../i18n/assist.json";
import answerBundle from "../../i18n/answer.json";

export const spa: typeof spaBundle = spaBundle;
export const S: typeof searchBundle = searchBundle;
export const AS: typeof assistBundle = assistBundle;
export const SA: typeof answerBundle = answerBundle;

/** `"{engine} failed"` interpolation — same `{name}` placeholders as Rust. */
export function fmt(
  template: string,
  values: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) =>
    values[name] != null ? String(values[name]) : "",
  );
}
