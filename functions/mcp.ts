// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// /mcp: the streamable-HTTP MCP endpoint, proxied like /api/*.

import { proxy, type PagesFn, type ProxyEnv } from "./_proxy";

export const onRequest: PagesFn<ProxyEnv> = ({ request, env }) =>
  proxy(request, env.API_ORIGIN);
