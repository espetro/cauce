// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Shared origin proxy for the CF Pages deployment (docs/deploy/cf-pages.md).
// The cauce SPA is fully same-origin — every runtime call is a relative
// /api/* fetch — so the Pages project only needs these tiny pass-through
// functions: a `_redirects` 200-rewrite cannot target an external origin.
// They stream (SSE answer/search responses pass through unbuffered) and
// run inside the Workers free-tier invocation quota, not per page view.

export interface ProxyEnv {
  /** Public origin of the cauce backend, e.g. `https://api.cauce.fyi`. */
  API_ORIGIN: string;
}

export interface FunctionCtx<E> {
  request: Request;
  env: E;
}

export type PagesFn<E> = (
  ctx: FunctionCtx<E>,
) => Promise<Response> | Response;

export function proxy(request: Request, apiOrigin: string): Promise<Response> {
  const incoming = new URL(request.url);
  const target = new URL(apiOrigin);
  target.pathname = incoming.pathname;
  target.search = incoming.search;

  const upstream = new Request(target.toString(), request);

  // Client-IP attribution: this function re-fetches through a second CF
  // edge (the api.* hostname), where `CF-Connecting-IP` arrives as this
  // worker's egress IP — so the real caller IP is carried in
  // `X-Forwarded-For` and the origin is configured with
  // `rate_limit.client_ip_header = "x-forwarded-for"`. Without that the
  // whole public instance would share one rate-limit / daily-budget
  // bucket. The stale `cf-connecting-ip` is dropped so nothing can read
  // it as the client's.
  const clientIp = request.headers.get("cf-connecting-ip");
  if (clientIp) {
    upstream.headers.set("x-forwarded-for", clientIp);
  }
  upstream.headers.set(
    "x-forwarded-proto",
    incoming.protocol.replace(":", ""),
  );
  upstream.headers.set("x-forwarded-host", incoming.host);
  upstream.headers.delete("cf-connecting-ip");

  return fetch(upstream);
}
