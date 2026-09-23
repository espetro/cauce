# fix: exec protocol v2 — forward safesearch/time_range/params (issue #88)

Issue: https://github.com/espetro/cauce/issues/88 (`feature`)
Branch: `v3/fix-88-exec-forward-params` — worktree `~/.worktrees/oxe-fix-88`

## Defect

Protocol v1 sends only `query`, `page`, `lang`, `timeout_ms`. `safesearch`,
`time_range`, and per-engine params are validated and cache-keyed but silently
dropped before they reach an exec child.

## Shape (protocol v2)

```
→ {"v":2,"query":"...","page":1,"lang":"en","timeout_ms":1500,
   "safesearch":"moderate","time_range":"week","params":{"k":"v"}}
← {"v":2,"results":[...],"error":null}
```

- New request fields, all absent under v1: `safesearch`
  (`off|moderate|strict`), `time_range` (`day|week|month|year`, optional),
  `params` (string map, optional; source is a new `[engines.params]` config
  table — SearchRequest has no per-engine params today).
- Capability negotiation: the parent speaks v2 optimistically to a fresh
  child. A v1 child rejects the line with `parse:unsupported protocol
  version: 2` (reference SDK wording); on any `error` containing
  "protocol version" the parent downgrades that *process* to v1 and resends
  the request with v2 fields omitted. The version is cached on `ChildIo`, so
  the probe costs v1 children one extra round trip per process lifetime.
- v2 children must accept `v:1` requests (a strict subset) and echo the
  request's `v` in the response, so old parents keep working against new
  children and the response `v` confirms the negotiated level.
- Parent accepts response `v` in `1..=2`; anything else is still
  `Transport("exec protocol version N unsupported")`.

## Do

- `crates/cauce-engines/src/exec.rs`: `PROTOCOL_VERSION = 2`,
  `MIN_PROTOCOL_VERSION = 1`, `ExecSpec.params`, `ExecRequest` v2 fields,
  `ChildIo.version`, downgrade-retry inside the existing `budget` timeout,
  `decode` accepts `v` in `1..=2`.
- `crates/cauce-core/src/config.rs`: `EngineEntry.params` (`[engines.params]`
  TOML table, `BTreeMap<String,String>` like `env`).
- `crates/cauce-engines/src/factory.rs`: forward `entry.params`.
- `sdk/python/cauce_engine_sdk/__init__.py`: `PROTOCOL_VERSION = 2`,
  `MIN_PROTOCOL_VERSION = 1`, `Request` gains `safesearch`/`time_range`/
  `params`/`v`, responses echo the request version.
- `sdk/python/cauce_engine_sdk/ddgs_auto.py`: map `safesearch`
  (strict→"on") and `time_range` (day|week|month|year→d|w|m|y) plus
  `**req.params` into `DDGS().text(...)`.
- Tests: echo fixture tags snippet with the received params; new
  `v1_engine.py` fixture for the downgrade path; SDK unit tests for v2
  parsing and version echo.
- Docs: `sdk/python/README.md` protocol section, `decisions.md` note.

Concurrency: #120 lands in the same files (exec.rs empty→NoResults,
ddgs_auto no_results emission). This diff stays additive around the
request-encode/negotiation path and does not refactor shared arms.
