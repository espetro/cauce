# Install

v3 is a single Rust binary, `oxe`. There is no package release yet; install
from a checkout.

## Build and install

```bash
git clone https://github.com/espetro/oxe
cd oxe
mise install            # Rust toolchain + cargo-deny + cargo-nextest
cargo install --locked --path crates/oxe-cli
```

`cargo install` puts `oxe` on `~/.cargo/bin`. Verify:

```bash
oxe serve --help
```

## Run

```bash
oxe serve                # UI + API + MCP on http://127.0.0.1:4479
oxe serve --headless     # API + MCP only
oxe mcp                  # stdio MCP transport, no HTTP listener
```

Defaults: loopback bind on port 4479 (`--bind`/`--port` or
`OXE_SERVER_HOST`/`OXE_SERVER_PORT` override), config at
`~/.config/oxe/config.toml`, data and logs under `~/.local/share/oxe/`.

The default engine set is the `ddgs` exec bridge plus every enabled
declarative spec (`bing`, `brave`; `wikipedia` ships `enabled: false`).
Pin a set with `OXE_ENGINES=bing,brave` or `[[engines]]` entries in
`config.toml`, e.g. to enable wikipedia:

```toml
[[engines]]
id = "wikipedia"
kind = "declarative"
enabled = true
```

## Supervision (oxmgr / launchd)

oxmgr supervises long-running apps on this machine; it owns the launchd
entries. Replace the v2 entry (`command = ".../uv/tools/oxe/bin/oxe"`) in
`~/.config/oxmgr/oxfile.toml` with the v3 binary:

```toml
[[apps]]
name = "oxe"
command = "/Users/<you>/.cargo/bin/oxe serve"
restart_policy = "always"
crash_restart_limit = 5
stop_timeout = 10
health_cmd = "curl -fsS --max-time 4 http://127.0.0.1:4479/health"
health_interval = 30
health_timeout = 5
health_max_failures = 3
```

Notes:

- `command` needs the absolute path to the installed binary; oxmgr does
  not expand `~`.
- Keep the port at the default 4479 so the existing health check and the
  portless alias below keep working.
- The v2 entry's `env = { OXE_BACKENDS = ... }` can be dropped; v3
  ignores it.
- The bundled `ddgs` exec engine only works when `oxe` runs from the
  repo checkout (its relative `sdk/python/...` path resolves by walking
  up from the process cwd) and its `uv` venv exists (`uv sync --project
  sdk/python --extra ddgs`). Under supervision, either pin the
  declarative engines and drop ddgs:

  ```toml
  env = { OXE_ENGINES = "bing,brave" }
  ```

  or keep ddgs by adding a `[[engines]]` block in
  `~/.config/oxe/config.toml` with absolute `command`/`args` pointing at
  the venv's python and `ddgs_auto.py`.

Apply the single app (never restart the daemon for a per-app change):

```bash
oxmgr apply ~/.config/oxmgr/oxfile.toml --only oxe
curl -fsS http://127.0.0.1:4479/health
```

## Portless alias

`portless` exposes the loopback service as `https://search.localhost`:

```bash
portless alias search 4479 --force
portless list | grep search   # confirm the route
```

This is a fixed-port supervised service, so use `portless alias`, not
`portless <name> <cmd>` (which is for agent-started dev servers).

## Agent wiring

MCP and HTTP wiring snippets for Claude Code, Hermes, maki and curl live
in [agents.md](agents.md).
