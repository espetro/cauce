# cauce

Local metasearch for humans and agents: one web UI, one HTTP API, one MCP server, one shared
TTL cache. Tail-tolerant fan-out over pluggable search engines, SQLite storage, single binary.

v3 is under construction. Plan: `.agents/plans/2026-09-21-v3-rust-core.md`.
Previous attempts: branches `legacy` (v1) and `v2-legacy` (v2).

Filing a bug? `GET /api/report` or `cauce report` produces a redacted JSON bundle to
attach — see [docs/reporting.md](docs/reporting.md).
