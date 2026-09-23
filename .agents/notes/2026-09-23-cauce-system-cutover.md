# 2026-09-23: cauce system-wide cutover

- `cargo install --path crates/cauce-cli` -> `~/.cargo/bin/cauce`; `~/.cargo/bin/oxe` removed.
- oxmgr: deleted stale `oxe` app (id 57, supervised `oxe serve`), `apply --only cauce` started app 58 `cauce serve` on 4479. Health: healthy.
- Config/data migrated: `~/.config/oxe/` left as backup; `~/.config/cauce/config.toml` carries openrouter [ai] + ddgs disabled (repo-relative sdk path can't run under oxmgr). `oxe.db` copied to `~/.local/share/cauce/cauce.db`; smoke-test db discarded.
- Verified: all 8 UI pages + /opensearch.xml 200, /api/search live network results (bing/brave embedded specs), MCP initialize -> "cauce" via :4479 and https://search.localhost/mcp, bifrost gateway exposes search-search_web/-exa_search/-cache_status/-cache_invalidate.
- Built-in search disabled: Claude Code `WebSearch` deny in ~/.claude/settings.json; Devin `disabled_tools: ["web_search"]` in ~/.config/devin/config.json; hermes already had `search` in disabled_toolsets + web.search_backend ""; maki has no builtin (uses gateway search-exa_search).
- ~/SEARCH.md rewritten for cauce naming, added /opensearch.xml + /api/suggest + browser search-provider instructions.
- REMAINING owner step for full rename: `gh repo rename oxe cauce` + update git remotes (binary/supervisor side now done).
