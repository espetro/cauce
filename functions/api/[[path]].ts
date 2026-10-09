// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// `[[path]]` catch-all: /api/* -> API_ORIGIN/api/* (all methods; POST
// bodies and SSE responses stream through).

import { proxy, type PagesFn, type ProxyEnv } from "../_proxy";

export const onRequest: PagesFn<ProxyEnv> = ({ request, env }) =>
  proxy(request, env.API_ORIGIN);
