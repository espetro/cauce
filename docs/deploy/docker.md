# Docker

```bash
docker build -t cauce .
docker run -p 127.0.0.1:4479:4479 -v cauce-data:/var/lib/cauce cauce
```

The image (`Dockerfile`, root) is `debian:bookworm-slim` + the release
binary + `python3`/`ddgs` for the bundled exec engine. It runs
**public-instance mode by default** (`CAUCE_SERVER_PUBLIC_INSTANCE=true`)
— set `[auth] admin_tokens` or accept that the ops surface is
unreachable. Everything is env-overridable; mount a real config at
`/etc/cauce/config.toml` for the `[[engines]]` blocks env can't express.

Multi-arch: `docker buildx build --platform linux/amd64,linux/arm64 -t
cauce --push .` — native-compile per platform under emulation, no
cross-toolchain. `linux/arm64` is the RPi/VPS-ARM tag.

## Compose (reference stack)

`docker-compose.yml` runs cauce + an optional `caddy` profile:

```bash
docker compose up cauce                    # app alone on 127.0.0.1:4479
DOMAIN=search.example.com docker compose --profile caddy up -d
```

The caddy service is the *reference* proxy — swap in nginx/traefik/a
cloudflared sidecar without touching the app. The compose file already
sets `CAUCE_RATE_LIMIT_TRUST_PROXY_HEADERS=true` because any
compose-network proxy hides the real peer.

## Slimmer images

The python3 layer exists only for the bundled `ddgs` exec engine.
Dropping `python3` + the `ddgs` install and disabling the engine
(`CAUCE_ENGINES=bing,brave` or a config `[[engines]]` block) yields a
~50 MB image with every declarative engine intact — exec engines are
the only feature that needs a runtime interpreter.

## Volumes and config

- `cauce-data` → `/var/lib/cauce` (`cauce.db`, logs). Anonymous volume
  is fine — it is a TTL cache.
- Config by env for knobs (`CAUCE_SERVER_*`, `CAUCE_RATE_LIMIT_*`,
  `CAUCE_EDGE_*`, `CAUCE_AI_*`, `CAUCE_ENGINES`) or a read-only bind
  mount for a full `config.toml`.
