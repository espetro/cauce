# Sizing a public cauce instance

Measured with `mise run loadtest` (`contrib/loadtest.sh`): oha against
`cauce serve` + the `replay` engine — deterministic, no upstream network —
so the numbers isolate the app's own cost: fan-out, dedup, cache, SQLite.
Real upstreams only *add* latency (better for shedding, worse for CPU).

Dev box: 8 vCPU x86-64, release build, `CAUCE_LOG=warn`.

## What the numbers say

| Scenario | Setup | Result |
|---|---|---|
| Spike shed | `max_inflight=32`, replay latency 300ms, 200 conns, unique queries, 8s | **8% 200 / 91% 429 / 0% 5xx**, p99 ≈ 2.5s — saturation surfaces only as `429 + Retry-After`, never an error page or hang |
| In-flight collapse | `fail_every=2` replay (a 2nd engine call 500s), latency 300ms, 50 identical cold queries | **50/50 200** — one engine call served the whole burst; singleflight holds e2e |
| Constrained | server pinned to **1 CPU** (`taskset -c 0`), uncached unique queries | **~1265 rps sustained**, RSS **~82 MB**, p99 22ms, 0 errors |
| Edge basis | `[edge]` defaults, replay latency 200ms | `Cache-Control: public, max-age=60, s-maxage=60` on shared routes; cold 204ms → warm hit **p50 1ms** |

## Capacity math for operators

Two ceilings, whichever binds first:

1. **Origin rps**: ~1265 rps per vCPU for *uncached* search traffic
   (replay — pure app cost). Upstream-bound deployments are far cheaper:
   the engine latency dominates and the CPU mostly waits, so the same
   core absorbs *more* requests/sec of real traffic, not fewer.
2. **Working set**: ~82 MB RSS under sustained 1-CPU load. Fits an RPi 4/5,
   a 512 MB VPS, or a Lambda 256 MB env with headroom — per
   `Resources::detect` the binary already self-limits fan-out on smaller
   hosts; `server.max_inflight` caps the queue above that.

**Off-origin absorption** is the multiplier. With edge `s-maxage=60` +
`stale-while-revalidate`, a share `a` of repeat traffic inside the TTL
window never reaches the app:

    effective origin rps = incoming rps × (1 − a)

where `a` is driven by your query-repeat ratio, not by cauce — popular
public instances see high `a` on trending queries. A warm hit costs ~1ms
at the app and ~0 at a CDN edge, so the free-tier CF-zone layout in
[cf-zone.md](cf-zone.md) converts `a` directly into unmetered requests.

## Spike behavior

The 8s window above is deliberately abusive (200 conn against 32 slots at
300ms upstream). The contract that held:

- Only `200` and `429` — the shed is explicit (`429 + Retry-After`), never
  a 5xx, timeout, or silent drop.
- Hot keys collapse *before* the limiter sees a problem: 50 concurrent
  identical queries = 1 engine call.
- p99 under saturation ≈ engine latency + queueing delay, then 429s —
  latency degrades gracefully instead of connection-starving.

## Reproduce

```sh
mise run loadtest               # full suite, ~40s
LOADTEST_DUR=15 contrib/loadtest.sh   # longer windows
LOADTEST_KEEP=1 contrib/loadtest.sh   # keep logs + oha JSON
```

Pinned-core run needs `taskset` (linux); elsewhere the constrained
scenario reports `CONST_PINNED=0` and the rps figure is unpinned.
