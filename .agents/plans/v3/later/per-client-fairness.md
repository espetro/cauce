# later: per-client fairness in admission
Issue: #47

Seam: `oxe-core::admission` (W1-07).
Trigger: a single client measurably starves others, e.g. one MCP client saturating the
queue blocks `ui` requests.
Shape: the admission queue becomes per-client round-robin keyed by `ClientKind` + MCP
client name, with a per-client concurrency cap (`admission.per_client_concurrency`,
default 4), `oxe_admission_wait_ms{client}`, and per-client queue depth on `/engines` and
`/dashboard`. Must not change the 429/stale overflow contract: on overflow serve a stale
row if one exists, else HTTP 429 + `Retry-After` / MCP `rate_limited{retry_after_s}`.
