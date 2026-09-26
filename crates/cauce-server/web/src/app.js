/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*
 * The bundled page script (`web/src/` -> `assets/app.js`, inlined by the
 * templates). Every `init*` is a no-op when its page's elements/flags are
 * absent, so one bundle serves all pages; each feature preserves the
 * inline script's gating.
 */
import { registerSseExtension } from "./sse.js";
import { initThemeToggle } from "./theme.js";
import {
  initAiModePill,
  initIndexBeacon,
  initSearchForm,
  initSearchStream,
} from "./search.js";
import { initAnswerPage } from "./answer.js";
import { initAssist } from "./assist.js";
import {
  initHashDetails,
  initHistoryCopy,
  initRequestIdCopy,
  initTracedCopy,
} from "./clipboard.js";
import { initEngineErrors } from "./engines.js";

registerSseExtension();
initThemeToggle();

const searchForm = document.getElementById("search-form");
if (searchForm) {
  initAiModePill(searchForm);
  initSearchForm(searchForm);
}
initIndexBeacon();
// W7-02: initAssist arms the card (non-streaming SERP) or waits for the
// stream's meta frame (streaming SERP) — hence it must exist before the
// search stream wires `cauce:sse`.
const assist = initAssist();
initSearchStream(document, window.S, assist);
initAnswerPage();

initRequestIdCopy();
initTracedCopy();
initHistoryCopy();
initHashDetails();
initEngineErrors();
