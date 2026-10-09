# Cloudflare zone in front of any origin

The req/mo play: a **plain CF zone** (proxied DNS, no Worker) in front
of any cauce origin — VPS, RPi behind cloudflared, Fly, Lambda Function
URL. Cache hits on CF's edge never reach the origin and cost nothing on
the free plan. This is the SearXNG-style public instance at ~€0–3.5/mo.

## What you get free

- **Edge cache**: cauce already stamps `Cache-Control: public,
  max-age=…, s-maxage=…` on shared responses (`/api/search`,
  `/api/suggest`, `/api/pages`, `/api/archive`, `/api/instance`, the
  `/app` fallbacks). With *Cache Rules → Respect origin TTL* (or just
  the standard tiered cache), repeat queries are served at the edge —
  the origin only sees cache misses.
- **One free WAF rate-limiting rule**: e.g. *API paths* —
  `(starts_with(http.request.uri.path, "/api/")) or
  (http.request.uri.path eq "/mcp")` → *Block, 10 req/10s per IP*.
  Covers abusive bots above the in-app bucket; keep the in-app
  `[rate_limit]` on anyway — it is the floor when the WAF rule can't
  express a shape.
- **Turnstile** (optional): siteverify on a challenge page for abusive
  query patterns; leave off by default — SearXNG-style instances are
  agent-facing and Turnstile blocks API clients too.
- **L7 DDoS absorption**: unmetered, always on.

## Origin-side config

```toml
[server]
public_instance = true

[rate_limit]
trust_proxy_headers = true   # CF sends CF-Connecting-IP; honoured first,
                             # before XFF — both are covered
```

Only proxied records (`orange cloud`) get the caching/WAF — a `grey
cloud` DNS-only record bypasses the whole point.

## What the edge does NOT cover

- `POST` anything (`/api/answer`, `/api/pages`, config, MCP JSON-RPC):
  always hits the origin. In-app `[rate_limit]` + `max_inflight` are
  the shed for those paths.
- `no-store`/private rows — admin, `/api/capabilities`: never stamped,
  never cached.
- Signed/personalized queries: if a future feature keys responses per
  user, those routes must stay `private` (the `RouteSpec` cache column
  is the enforcement point — see PUB-01).

## cloudflared alternative

On a home RPi/NAT box, skip DNS proxying and use `cloudflared tunnel`:
same free edge (it *is* the CF network), `CF-Connecting-IP` preserved,
no inbound ports, no public IP needed. The zone recipe above composes
with it — the tunnel lands on the same proxied zone.
