# Public instances (SearXNG-style)

Issue: #280 (PUB-01), #62 (auth — delivered by FX-07), #63 (deploy docs, PUB-02).
Context: research report + platform digests on #63.

## Goal and constraint

Make running a **public** cauce instance boring: a €3.5 VPS / Oracle free tier /
RPi behind a free Cloudflare zone should absorb SearXNG-fleet traffic without the
operator thinking about it. The binding constraint on the free platforms is
**requests/month at the edge → origin**: an edge cache hit never reaches cauce,
so the server's job is (a) be safe to expose, (b) mark its responses cacheable or
private correctly, (c) shed overload cleanly.

Vertical scaling only for now — `Resources::detect` already adapts sqlite tuning
and upstream concurrency to the host. Horizontal (shared cache, Postgres) is the
`later/` item, not a blocker here.

## What already landed (FX-07, #274)

`server.public_instance`, `[auth] admin_tokens`, the `RouteAuth::Admin` routes
column + `require_admin` middleware, `/api/capabilities` + `/api/instance`,
history/click writes gated off in public mode, admin-surface UX in the SPA.

## PUB-01 — public-instance readiness (this PR)

Seam: `cauce-server` middleware + the `ROUTES` table; `cauce-cli serve` bind
policy; `cauce-core::config` new sections.

1. **Non-loopback bind.** W1-13's refusal is replaced by the real auth boundary:
   `cauce serve --bind <non-loopback>` requires `server.public_instance = true`
   (without it the admin gate no-ops and the ops surface would be open). Empty
   `admin_tokens` stays allowed — fail-closed admin surface, per FX-07. Setting
   `auth.enabled` warns that it is superseded. Policy lives on `ServerConfig`
   (`check_bind`) so the rule is unit-tested in core, not buried in the CLI.
2. **Per-route cache class.** `RouteSpec` gains `cache: RouteCache`
   (`private` default, `shared`) — same reviewable-column pattern as `auth:`.
   A route layer stamps `Cache-Control` on `< 400` responses that don't already
   carry it (handler headers win — the SPA's immutable/no-cache split stands):
   - `shared` → `public, max-age=N, s-maxage=N, stale-while-revalidate=N`
     (`edge.ttl_s`, default 60)
   - `private` → `private, no-store` — makes auth-varying rows (`/api/capabilities`)
     and admin rows safe under a "cache everything" edge rule.
   Shared rows: `GET /api/search`, `GET /api/suggest`, `GET /api/archive`,
   `GET /api/pages/{url}`, `GET /api/instance`, `/opensearch.xml`,
   `/favicon.ico`, the `/` `/search` `/answer` permanent redirects,
   `/answer/{id}`, and the `/app*` rows (handlers override per-asset).
   New `[edge]` section: `enabled` (default true), `ttl_s` (default 60).
3. **`[rate_limit]` per-IP token bucket.** governor keyed limiter (already a dep
   via cauce-core). Scope: `/api/*` + `/mcp` — the embedded UI/assets are cheap
   and a page load is ~10 requests; counting them would throttle real users at
   any sane burst. Keys: `CF-Connecting-IP` / leftmost `X-Forwarded-For` when
   `trust_proxy_headers = true`, else the `ConnectInfo` peer. Loopback peers and
   `Role::Admin` requests are exempt; `enabled` defaults to `public_instance`
   (`enabled = false` opts out explicitly). Overflow → 429 `rate_limited` +
   `Retry-After` (the envelope shape already exists).
4. **`server.max_inflight`** (0 = off): a global in-flight `Semaphore` — the
   last line of defence on a 1-core/1 GB host. Saturated → 429 `overloaded` +
   `Retry-After`. `/health`, loopback and admin exempt (monitoring and the
   operator must still get in).
   `serve()` gains `into_make_service_with_connect_info::<SocketAddr>()` for the
   peer address.

Restart-required: `server.max_inflight`, `rate_limit.*`, `edge.*` — all frozen
at router build like `HostGuard`, so `PUT /api/config` cannot widen exposure.

## PUB-02 — deploy artifacts (#63)

Multi-arch Dockerfile (musl → distroless), compose, `contrib/cauce.service`,
`contrib/Caddyfile`, `docs/deploy/`: vps · docker · cf-zone (cache rules, WAF
rate limit, Turnstile) · lambda (LWA, response streaming). Optional
`aarch64-musl` release target for RPi.

## PUB-03 — per-user AI (BYOK)

`[ai] allow_user_keys` + optional per-request `ai` override
(`{api_key?, model?, protocol?}`) merged over `[ai]` on `POST /api/answer`;
`allow_user_base_url` stays off by default (an open base_url is an egress
relay). Free-AI mode adds `[ai] free_daily_answers` — a per-IP daily budget,
since answer runs cost real money. Creds in browser localStorage; precedence
request > `[ai]` > disabled.

## PUB-04 — load harness

One k6/oha script + `mise run loadtest` against `cauce serve` + replay engine:
spike shed (10× oversubscription → stale-serve/429 contract), singleflight
collapse e2e, cgroup ~1core/1GB sustained rps + RSS ceiling, edge-absorption
ratio. Results → `docs/deploy/sizing.md` (PUB-02 input).

## Out of scope

Workers/Fastly wasm ports (a CF zone in front of the binary already solves the
req/mo bottleneck), Postgres/multi-instance, Deno/Spin (ruled out by research).
