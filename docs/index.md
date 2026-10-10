# cauce docs

Local metasearch for humans and agents — one self-contained Rust binary
that serves a web UI, an HTTP API, and an MCP server over a shared TTL
cache.

These docs cover **using, configuring, and deploying** cauce. If you run
a `cauce serve` process — locally or as a public instance — this is the
place. Developer-facing internals (builds, UI specs, architecture) live
under **Developers Guide** (mirrored from `.agents/docs/` in the repo).

## Where to start

- **[Install](install/)** — release binary or `cargo install`, then `cauce serve`.
- **[Agents](agents/)** — point Claude Code, Hermes, or any MCP client at `/mcp`.
- **[Deploy](deploy/)** — VPS, Cloudflare Pages + tunnel, Docker, Lambda, and sizing.
- **[Reporting](reporting/)** — generate a self-contained diagnostics bundle.

## Getting help

Bugs and ideas: <https://github.com/espetro/cauce/issues>. The `--help`
output of each subcommand is the ground truth for flags.
