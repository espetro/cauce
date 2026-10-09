# Web UI on Cloudflare Pages + backend on a VPS

The reference "split" deployment: the SPA is served from Cloudflare
Pages (static assets, unmetered), and `cauce serve` runs on a VPS behind
cloudflared as the API origin. The SPA is fully same-origin — every
runtime call is a relative `/api/*` fetch — so Pages fronts the API too
via a handful of tiny proxy functions in the repo-root `functions/`
directory.

```
browser ──▶ cauce.fyi (CF Pages)
              ├── /app/*, /          → static assets (free, unmetered)
              ├── /api/*, /mcp,      → functions/* → https://api.cauce.fyi
              │   /favicon.ico,         (Workers free tier: ~100k req/day,
              │   /opensearch.xml,      shared across all functions)
              │   /health
              └── api.cauce.fyi (cloudflared) ──▶ cauce serve on the VPS
```

Why functions and not `_redirects`: a `200` rewrite can only proxy
*relative* paths on the same site — external origins are unsupported.
The functions are a ~10-line pass-through each and stream, so SSE
answers (`/api/answer`, `/api/search/stream`) work unchanged.

## Pages project setup

1. **Workers & Pages → Create → Pages → Connect to Git** → pick the repo.
2. Build settings:
   - **Build command:** `bash deploy/cf-pages/build.sh`
   - **Build output directory:** `pages-dist`
3. **Environment variable** (Production + Preview):
   `API_ORIGIN=https://api.cauce.fyi` — the backend's public origin.
4. **Custom domain:** `cauce.fyi` (CNAME is set up automatically when the
   zone is on Cloudflare).

`build.sh` runs `pnpm install` + `build:spa` (the same vite build the
embedded-assets pipeline uses), then assembles `pages-dist/` with the
SPA at `/app/`, a root redirect `/ → /app/`, and the root `index.html`
Pages' built-in SPA fallback needs for deep links like `/app/search?q=`.

## Backend side (the VPS)

Deploy `cauce serve` per [vps.md](vps.md) (systemd unit) or
[docker.md](docker.md), fronted by cloudflared at `api.cauce.fyi`. On top
of the public-instance base config, one extra key is **required** for
this topology:

```toml
[server]
public_instance = true
public_origin   = "https://cauce.fyi"   # used in opensearch.xml etc.

[rate_limit]
trust_proxy_headers = true
client_ip_header    = "x-forwarded-for"
```

`client_ip_header` matters because this is a *chained* CDN deployment:
the Pages function re-fetches through a second Cloudflare edge
(`api.cauce.fyi`), where `CF-Connecting-IP` arrives as the function's
egress IP — not the user's. The function puts the real caller IP in
`X-Forwarded-For`; without this override, every visitor would share a
single rate-limit and daily-answer budget.

Admin/BYOK `Authorization` headers pass straight through the proxy, so
`admin_tokens` and `ai.allow_user_keys` work unchanged — user keys never
touch Cloudflare beyond the TLS hop that already carries the request.

## Quotas to keep in mind

- Static assets (`/app/*`, `/`, hashed bundles): unmetered.
- Pages Functions: the Workers free plan (~100k invocations/day on the
  standard quota). Every `/api/*` call costs one invocation — a metasearch
  UI makes a few per search, so this is generous for hobbyist fleets but
  is the ceiling to watch.
- The backend VPS sees only proxied API traffic: `origin_rps` follows
  the sizing formula in [sizing.md](sizing.md) with `repeat_share ≈ 0`
  for API calls (the edge-cache win here is that Pages users never hit
  the origin for assets at all).
