# Install

v3 is a single self-contained Rust binary, `cauce` (engines + UI + AI +
archive compiled in; rustls, no OpenSSL). Install from a GitHub release or
build from a checkout.

## Install a release binary

Pick the tarball for your platform under
<https://github.com/espetro/cauce/releases> (Linux x86_64 glibc and musl,
Linux aarch64, macOS arm64 and x86_64), e.g.:

```bash
curl -LO https://github.com/espetro/cauce/releases/download/v0.5.0/cauce-v0.5.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf cauce-v0.5.0-x86_64-unknown-linux-gnu.tar.gz
./cauce serve     # UI + API + MCP on http://127.0.0.1:4479
```

Verify the binary reports the release version:

```bash
./cauce --version
```

macOS removes the quarantine attribute before first run:

```bash
xattr -d com.apple.quarantine ./cauce
```

The bundled `ddgs` exec engine is not in the release binaries — it needs a
source checkout and python3 (`uv sync --project sdk/python --extra ddgs`),
so it stays a custom-build option. Disable it in `config.toml` with the
`[[engines]]` block shown below.

## Build and install

```bash
git clone https://github.com/espetro/cauce
cd cauce
mise install            # Rust toolchain + cargo-deny + cargo-nextest
cargo install --locked --path crates/cauce-cli
```

`cargo install` puts `cauce` on `~/.cargo/bin`. Verify:

```bash
cauce serve --help
```

## Run

```bash
cauce serve                # UI + API + MCP on http://127.0.0.1:4479
cauce serve --headless     # API + MCP only
cauce mcp                  # stdio MCP transport, no HTTP listener
```

Defaults: loopback bind on port 4479 (`--bind`/`--port` or
`CAUCE_SERVER_HOST`/`CAUCE_SERVER_PORT` override), config at
`~/.config/cauce/config.toml`, data and logs under `~/.local/share/cauce/`.

When serving browser discovery URLs behind TLS or a portless proxy, set the
canonical external origin in `[server]`. It is used for the absolute URLs in
`/opensearch.xml`; request `Host` and forwarded headers are not trusted for
this purpose. If omitted, the configured bind host and port are used over HTTP.

```toml
[server]
public_url = "https://search.localhost"
```

The default engine set is the `ddgs` exec bridge plus every enabled
declarative spec (`bing`, `brave`; `wikipedia` ships `enabled: false`).
Pin a set with `CAUCE_ENGINES=bing,brave` or `[[engines]]` entries in
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
name = "cauce"
command = "/Users/<you>/.cargo/bin/cauce serve"
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
- The bundled `ddgs` exec engine only works when `cauce` runs from the
  repo checkout (its relative `sdk/python/...` path resolves by walking
  up from the process cwd) and its `uv` venv exists (`uv sync --project
  sdk/python --extra ddgs`). Under supervision, disable it in
  `~/.config/cauce/config.toml`:

  ```toml
  [[engines]]
  id = "ddgs"
  kind = "exec"
  enabled = false
  command = "python3"
  args = ["sdk/python/cauce_engine_sdk/ddgs_auto.py"]
  ```

  (`CAUCE_ENGINES` cannot name `bing`/`brave`: the pin validates against
  `[[engines]]` entries and built-ins, and embedded specs are only
  auto-registered when the pin is unset.) To keep ddgs instead, give its
  `[[engines]]` block absolute `command`/`args` pointing at the venv's
  python and `ddgs_auto.py`.

Apply the single app (never restart the daemon for a per-app change):

```bash
oxmgr apply ~/.config/oxmgr/oxfile.toml --only cauce
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
