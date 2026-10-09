# Deploying cauce

cauce is one binary (`cauce serve` — UI + API + MCP). Pick the shape
that fits; everything here is a **reference stack**, not a requirement —
the app never dictates your proxy, platform, or datastore.

| Target | Guide | Shape |
| --- | --- | --- |
| VPS / RPi + Caddy or cloudflared | [vps.md](vps.md) | systemd unit, TLS at the edge |
| Docker / compose | [docker.md](docker.md) | multi-arch image, any proxy |
| Any origin + Cloudflare zone | [cf-zone.md](cf-zone.md) | free-tier edge cache + WAF |
| AWS Lambda | [lambda.md](lambda.md) | LWA image, SSE streaming |

## The public-instance contract

Every public deployment shares one config — `server.public_instance`
puts the ops surface (`/api/config`, engines admin, dashboards) behind
`[auth] admin_tokens`, turns off per-user state (history, clicks,
answer log), and defaults `[rate_limit]` on:

```toml
[server]
public_instance = true

[auth]
admin_tokens = ["<long-random-token>"]   # empty = ops unreachable

[rate_limit]
trust_proxy_headers = true               # REQUIRED behind any proxy
```

`trust_proxy_headers` matters wherever a proxy fronts the app (Caddy,
cloudflared, CF zone, ALB, Lambda): without it every client shares one
per-IP bucket keyed on the proxy's own address. Set it only when a
proxy you control actually sends `CF-Connecting-IP`/`X-Forwarded-For` —
spoofable otherwise.

## What cauce ships vs. what the platform ships

In-app, always available, no extra infra: per-IP token bucket,
in-flight cap (`server.max_inflight`), per-engine semaphore +
singleflight, `Cache-Control` stamped on shared routes (`[edge]`).

At the platform/proxy layer — cauce does not replace these: TLS, WAF
rules, edge caching of the stamped responses, DDoS absorption, queues
for write-heavy endpoints. The guides wire the free tier of each.

## Sizing

aarch64-gnu release runs on RPi 4/5-class hardware; idle RSS < 80 MB.
Load numbers land in [sizing.md](sizing.md) (PUB-04 — k6 spike shed,
singleflight collapse, edge-absorption ratio). Horizontal scaling is a
roadmap item (shared cache/coordination needs the Postgres `Store`,
issue #61); today: one instance, one volume.
