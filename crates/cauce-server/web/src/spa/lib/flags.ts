/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Build-time feature flags baked into the bundle by vite. `LANDING`
 * (`VITE_CAUCE_LANDING=1`, set by deploy/cf-pages/build.sh) produces the
 * cauce.fyi landing: the app's home layout with the product sections
 * scrolled below it (duckduckgo.com-style — the apex IS the app). The
 * binary's rust-embed build never sets it, so self-hosted `cauce serve`
 * always gets the plain app layout. It must be a build-time flag: the
 * Pages deploy has to render the landing even while the backend is
 * unreachable, so it cannot ride a runtime capabilities fetch.
 */
export const LANDING: boolean = import.meta.env.VITE_CAUCE_LANDING === "1";
