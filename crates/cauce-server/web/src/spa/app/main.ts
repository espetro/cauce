/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

// SPA entry (FX-02): mounts the shell. The router lands in FX-03
// (history mode; `/app/{*rest}` already falls back to this shell).

import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("spa mount point #app missing");
}

mount(App, { target });
