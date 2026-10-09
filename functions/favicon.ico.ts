// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// /favicon.ico: proxied so the origin's image/svg+xml content type is
// preserved (Pages would serve the same bytes as image/x-icon).

import { proxy, type PagesFn, type ProxyEnv } from "./_proxy";

export const onRequest: PagesFn<ProxyEnv> = ({ request, env }) =>
  proxy(request, env.API_ORIGIN);
