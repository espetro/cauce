# Support reports

When something goes wrong, cauce can export a single JSON **report bundle** that captures the
state a maintainer needs — no digging through directories, no log snippets pasted by hand.

## Getting a bundle

- **From the UI**: `/settings` → *Logging* → **Download support report**. This hits
  `GET /api/report`, which answers with a `Content-Disposition: attachment` file named
  `cauce-report-<timestamp>.json`.
- **From the API**: `GET /api/report?days=N` — `days` (default 7) bounds the stats window,
  the audit tail, and which log files are read. `?include_queries=1` (or `?verbose=1`)
  selects the verbose profile for that one request.
- **From the CLI**: `cauce report` writes `cauce-report-<timestamp>.json` in the current
  directory. Flags:

  | flag | effect |
  |---|---|
  | `--out PATH` | write the bundle there instead |
  | `--days N` | export window (default 7) |
  | `--include-queries` | verbose profile for this export only |
  | `--print` | emit the bundle on stdout instead of writing a file |
  | `--gh` | file the prefilled issue via `gh issue create` |

  The CLI assembles the bundle in-process from the same config, SQLite store and log
  directory the server uses — no running server needed.

## Where the raw logs live

The bundle's `errors_tail`/`storage` sections read the JSONL logs under
`<data_dir>/logs/cauce-YYYY-MM-DD.jsonl` — one file per day, kept for
`logs.retention_days` (default 30). `<data_dir>` is
`~/.local/share/cauce` by default, or `$CAUCE_DATA_DIR`. `cauce tail` and `cauce trace`
read the same files.

## What's in the bundle

Schema v1 (`v`, `profile`, `generated_at`, `notes`, `sections`):

- `cauce` — version, compiled features, bind address, uptime
- `config` — the effective config as `GET /api/config` shows it (already redacted)
- `stats` — the `/api/stats?days` aggregates
- `engines` — the `/api/engines` health views
- `audit_tail` — newest 200 audit rows in the window
- `errors_tail` — newest 100 warn/error log records in the window
- `storage` — DB file size, log file list, cache entry count
- `eval_latest` — the latest `cauce eval engines` report, if any

## Redaction: safe vs verbose

The **safe** profile is the default and applies on every export. It removes query text
(folds `query` fields to their `query_hash`), strips secrets (API keys, tokens,
passwords), drops URL credentials and query strings, and keeps only the standard actor
labels (`ui`, `api`, `cli`, `mcp:<name>`). Safe is what to attach to a public issue.

**Verbose** (`--include-queries` / `?include_queries=1`) keeps raw query text, for
debugging your own instance. It is a per-export opt-in and is never persisted. Secrets
and URL credentials are still scrubbed, but **do not file a verbose bundle publicly**.

## Sharing

Every `GET /api/report` response carries the `X-Report-Issue-Url` header: a prefilled
`github.com/espetro/cauce/issues/new` URL whose title and body summarize the bundle
(version, bind, uptime, searches in the window) and explain how to attach the file.
`cauce report` prints the same URL, and `--gh` shells out to `gh issue create` when `gh`
is on PATH.
