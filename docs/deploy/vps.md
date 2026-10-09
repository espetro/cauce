# VPS / bare metal / RPi

The reference stack: `cauce` under systemd on a cheap VPS or an RPi,
TLS at a proxy (Caddy) or a tunnel (cloudflared). ~€3.5/mo or a spare
board — the same shape the searx.space fleet runs.

## Install

```bash
# release tarball (aarch64-gnu for RPi OS / Ubuntu ARM)
tar -xzf cauce-<ver>-<target>.tar.gz
sudo install -m755 cauce /usr/local/bin/cauce

sudo useradd --system --home-dir /var/lib/cauce --shell /usr/sbin/nologin cauce
sudo install -d -o cauce -g cauce /var/lib/cauce /etc/cauce
```

`/etc/cauce/config.toml`:

```toml
[server]
public_instance = true

[auth]
admin_tokens = ["<long-random-token>"]

[rate_limit]
trust_proxy_headers = true   # the proxy in front sets XFF — see README
```

Unit and boot:

```bash
sudo install -m644 contrib/cauce.service /etc/systemd/system/cauce.service
sudo systemctl daemon-reload && sudo systemctl enable --now cauce
journalctl -u cauce -f        # "addr:" line reports the bind
```

## Front it — pick one

**Caddy** (direct exposure, TLS automatic): install `contrib/Caddyfile`,
`DOMAIN=search.example.com caddy run --config contrib/Caddyfile`. Point
DNS at the box; open 80/443.

**cloudflared** (no ports open, works on RPi/CGNAT): `cloudflared tunnel`
routes `search.example.com` → `http://127.0.0.1:4479`. The socket stays
loopback; the tunnel forwards real client IPs via `CF-Connecting-IP`,
which `trust_proxy_headers` already honours.

**nginx/traefik/anything else**: `proxy_pass http://127.0.0.1:4479` +
`X-Forwarded-For` — nothing cauce-side is Caddy-specific.

## Ops

- Admin surface: `Authorization: Bearer <token>` — `/app/admin`, `PUT
  /api/config`, engine toggles, cache ops. With empty `admin_tokens` it
  stays unreachable (fail-closed).
- Data: `/var/lib/cauce/cauce.db` (WAL SQLite) — back it up or don't;
  it's a TTL cache plus the config file.
- Upgrades: replace the binary, `systemctl restart cauce`. The
  `[server] host/port/public_instance/max_inflight` and
  `rate_limit.requests_per_second/burst` keys are restart-bound; the
  rest apply via `PUT /api/config`.
- RPi notes: use the `aarch64-unknown-linux-gnu` tarball; the store is
  a TTL cache so SD-card wear is modest — `CAUCE_DATA_DIR` can point at
  tmpfs if you'd rather lose the cache on reboot than write to the card.
