<p align="center">
  <img src="assets/brand/cauce-mark-512.png" width="96" height="96" alt="cauce icon" />
</p>

<h1 align="center">cauce</h1>

<p align="center">Local metasearch for humans and agents. One binary, one shared TTL cache.</p>

<p align="center">
  <a href="https://github.com/espetro/cauce/releases"><img src="https://img.shields.io/github/v/release/espetro/cauce" alt="latest release" /></a>
  <a href="https://github.com/espetro/cauce/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue" alt="license: MPL-2.0" /></a>
  <a href="https://x.com/josocjoq"><img src="https://img.shields.io/twitter/follow/josocjoq?style=social" alt="Follow @josocjoq" /></a>
</p>

## Try it now

[cauce.fyi](https://cauce.fyi) — the public instance · docs at [docs.cauce.fyi](https://docs.cauce.fyi)

## Install it

```sh
mise use -g github:espetro/cauce
cauce serve        # UI + API + MCP on http://127.0.0.1:4479
```

Or grab a release binary / build from source: [docs/install.md](docs/install.md)

## What it does

- One web UI, one HTTP API, one MCP server — all reading the same TTL cache.
- Tail-tolerant fan-out over pluggable engines (declarative YAML, exec, replay) with hedging and circuit breakers.
- Grounded AI answers at `POST /api/answer`, with optional BYOK and a free daily-answer budget for public instances.
- Local archive + search history in SQLite FTS5.
- Self-hosted and local-first: a single Rust binary, loopback by default; a public-instance mode exists for the edge.

## Stack

| Path                        | What it owns                                            |
| --------------------------- | ------------------------------------------------------- |
| `crates/cauce-core`         | Search pipeline, `Store` + `Engine` traits              |
| `crates/cauce-server`       | axum HTTP/SSE/MCP, embedded Svelte SPA at `/app`        |
| `crates/cauce-engines`      | Declarative / exec / replay engine runtimes             |
| `crates/cauce-store-sqlite` | rusqlite `Store` implementation                         |
| `crates/cauce-agent`        | Grounded-answer agent loop                              |
| `crates/cauce-cli`          | `cauce serve` / `search` / `engine` / `cache` / `record` |
| `engines/`                  | YAML engine specs + fixtures                            |
| `sdk/python`                | exec-protocol SDK + `ddgs` reference engine             |

End-user docs: [docs.cauce.fyi](https://docs.cauce.fyi) · developer docs: [`.agents/docs/`](.agents/docs/)

Filing a bug? `GET /api/report` or `cauce report` produces a redacted JSON bundle to attach — see [docs/reporting.md](docs/reporting.md).
