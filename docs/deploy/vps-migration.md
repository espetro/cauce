# Migrating cauce.fyi from Cloudflare Pages to VPS (cloudflared Tunnel)

## Overview

The initial deployment used a split topology:
- **Cloudflare Pages (`cauce.fyi`)**: Hosted the compiled static SPA and routed API requests to the VPS backend via edge functions (`functions/`).
- **VPS (`api.cauce.fyi`)**: Ran `cauce serve` behind a Cloudflare Tunnel (`cloudflared`).

### Why Migrate Completely to VPS

1. **Eliminates Soft-404 Issues**: Cloudflare Pages' static server catches non-file URLs and serves `index.html` (HTTP 200 `text/html`). This breaks automated agent discovery scanners probing standard paths (e.g. `/.well-known/*`, `auth.md`).
2. **Eliminates Edge Functions**: Removes maintenance of `functions/` edge proxies, avoiding the 100k invocations/day free tier quota.
3. **Single Unified Origin**: Eliminates dual-origin CORS complexity (`cauce.fyi` vs `api.cauce.fyi`), simplifies cookies/tokens, and prevents rate-limit IP obfuscation from chained CDN hops.
4. **Native Feature Support**: `cauce serve` on the VPS already embeds the entire Svelte SPA via `rust-embed`, handles RFC 8288 `Link` headers, markdown content negotiation, SSE streaming, and all discovery endpoints natively.

---

## Current Cloudflare Infrastructure Discovery

- **Cloudflare Account ID**: `e4f07b24a61b388bbcaadca4d41cd6cd` (`conta-personal`)
- **Zone**: `cauce.fyi` (`99dc213d2e975d9a0e44bcb3af77f5d4`)
- **Tunnel**: `vps-tunnel` (`1dad9cd2-857b-421f-822c-a2bc2cf7f389`), status `healthy`, remotely managed.
- **Current Ingress**:
  ```json
  [
    { "hostname": "tarima.illo.fyi", "service": "http://127.0.0.1:3001" },
    { "hostname": "api.cauce.fyi", "service": "http://127.0.0.1:4479" },
    { "service": "http_status:404" }
  ]
  ```

---

## Migration Steps

### Step 1: Add `cauce.fyi` to the Cloudflare Tunnel Ingress

Update the tunnel configuration so `cauce.fyi` forwards directly to `cauce serve` on port 4479:

```bash
curl -X PUT "https://api.cloudflare.com/client/v4/accounts/e4f07b24a61b388bbcaadca4d41cd6cd/cfd_tunnel/1dad9cd2-857b-421f-822c-a2bc2cf7f389/configurations" \
  -H "Authorization: Bearer <CLOUDFLARE_API_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "ingress": [
        {
          "hostname": "tarima.illo.fyi",
          "service": "http://127.0.0.1:3001",
          "originRequest": { "httpHostHeader": "tarima.illo.fyi" }
        },
        {
          "hostname": "cauce.fyi",
          "service": "http://127.0.0.1:4479",
          "originRequest": { "httpHostHeader": "cauce.fyi" }
        },
        {
          "hostname": "api.cauce.fyi",
          "service": "http://127.0.0.1:4479",
          "originRequest": { "httpHostHeader": "api.cauce.fyi" }
        },
        {
          "service": "http_status:404"
        }
      ],
      "warp-routing": { "enabled": false }
    }
  }'
```

### Step 2: Remove `cauce.fyi` Custom Domain from Cloudflare Pages

Remove the domain binding from the `cauce` Pages project so Pages releases the apex hostname:

```bash
# Domain ID: 232d3ce7-5f53-4100-9dc5-5801f3ec8584
curl -X DELETE "https://api.cloudflare.com/client/v4/accounts/e4f07b24a61b388bbcaadca4d41cd6cd/pages/projects/cauce/domains/232d3ce7-5f53-4100-9dc5-5801f3ec8584" \
  -H "Authorization: Bearer <CLOUDFLARE_API_TOKEN>"
```

### Step 3: Route `cauce.fyi` in Cloudflare DNS

In the Cloudflare dashboard (or via DNS API), configure the DNS record for `cauce.fyi`:
- **Type**: `CNAME`
- **Name**: `cauce.fyi` (or `@`)
- **Target**: `1dad9cd2-857b-421f-822c-a2bc2cf7f389.cfargotunnel.com`
- **Proxy status**: Proxied (`orange-clouded`)

*(Note: Cloudflare automatically flattens apex CNAMEs to Cloudflare anycast IPs with SSL/TLS active).*

### Step 4: Verify VPS Configuration

In `/etc/cauce/config.toml` on the VPS:

```toml
[server]
public_instance = true
public_url      = "https://cauce.fyi"
allowed_hosts   = ["cauce.fyi", "api.cauce.fyi"]

[rate_limit]
trust_proxy_headers = true
# Cloudflared forwards real client IP directly in CF-Connecting-IP
```

Restart the service:
```bash
sudo systemctl restart cauce
```

### Step 5: Verification

1. Verify apex answers directly from `cauce serve`:
   ```bash
   curl -is https://cauce.fyi/health
   # Must return 200 {"status":"ok","version":"0.9.2"}
   ```
2. Verify discovery endpoints return correct JSON (not HTML):
   ```bash
   curl -is https://cauce.fyi/.well-known/api-catalog
   # Must return 200 application/linkset+json
   ```
3. Verify unknown `.well-known` paths return 404 JSON:
   ```bash
   curl -is https://cauce.fyi/.well-known/nonexistent
   # Must return 404 application/json {"error":"not_found",...}
   ```
4. Verify SPA loads from `/app`:
   ```bash
   curl -is https://cauce.fyi/app
   # Must return 200 text/html with WebMCP tool registration
   ```
