#!/usr/bin/env bash
# cauce public-instance load harness (PUB-04). Small and pointed: prints
# KEY=VALUE metrics for docs/deploy/sizing.md. Uses `oha` (mise-managed),
# the `replay` engine (deterministic, fault-injected), and curl.
#
# Scenarios:
#   spike_shed   10x oversubscription on a slow engine -> 200/429 contract, zero 5xx
#   collapse     concurrent identical cold queries collapse to ~1 engine call
#                (proved via CAUCE_REPLAY_FAIL_EVERY: a second call would 5xx)
#   constrained  server pinned to 1 CPU: sustained uncached rps + RSS ceiling
#   edge_basis   Cache-Control s-maxage present + cold-vs-warm latency for the
#                edge-absorption math in sizing.md
set -euo pipefail

BIN="${CAUCE_BIN:-target/release/cauce}"
PORT_BASE="${LOADTEST_PORT_BASE:-24480}"
DUR="${LOADTEST_DUR:-8}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

oha="${OHA:-$(command -v oha || true)}"
[ -z "$oha" ] && oha="$(command -v mise >/dev/null && mise exec -- oha --version >/dev/null 2>&1 && echo "mise exec -- oha")"
[ -z "$oha" ] && { echo "oha not found: mise use -g ubi:hatoo/oha" >&2; exit 1; }
$oha --version >/dev/null 2>&1 || oha="mise exec -- oha"

TMP="$(mktemp -d /tmp/cauce-loadtest.XXXXXX)"
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done
  if [ "${LOADTEST_KEEP:-0}" = 1 ]; then echo "kept $TMP"; else rm -rf "$TMP"; fi }
trap cleanup EXIT
PIDS=""
PORT=$PORT_BASE

serve() { # serve <extra env as k=v ...> ; TASKSET_CMD prefixes the launch (e.g. "taskset -c 0")
  PORT=$((PORT + 1))
  local dir="$TMP/s$PORT"; mkdir -p "$dir/data" "$dir/cfg"
  ${TASKSET_CMD:-} env CAUCE_DATA_DIR="$dir/data" CAUCE_CONFIG_DIR="$dir/cfg" \
      CAUCE_ENGINES=replay CAUCE_LOG=warn \
      CAUCE_SERVER_PORT=$PORT CAUCE_SERVER_PUBLIC_INSTANCE=true \
      CAUCE_RATE_LIMIT_ENABLED=false "$@" \
      "$BIN" serve --bind 127.0.0.1 >"$dir/log" 2>&1 &
  local pid=$!
  for _ in $(seq 50); do
    curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && { echo "$pid $PORT"; return; }
    sleep 0.2
  done
  echo "serve $PORT failed:" >&2; tail -5 "$dir/log" >&2; exit 1
}
stop() { kill "$1" 2>/dev/null || true; wait "$1" 2>/dev/null || true; }

cat >"$TMP/stats.py" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
sc = d.get("statusCodeDistribution", {})
n = max(1, sum(sc.values()))
ok = sc.get("200", 0)
shed = sc.get("429", 0)
err = sum(v for k, v in sc.items() if k.startswith("5"))
edist = d.get("errorDistribution", {})
en = sum(edist.values())
sys.stderr.write("  statuses=%s errors=%s\n" % (sc, edist if en else ""))
lat = d.get("latencyPercentiles") or {}
mode = sys.argv[2] if len(sys.argv) > 2 else "stats"
if mode == "rps":
    print(int(d.get("summary", {}).get("requestsPerSec", 0)))
else:
    print("%d %d %d %.0f %.0f" % (
        ok * 100 // n, shed * 100 // n, err * 100 // n,
        (lat.get("p50") or 0) * 1000, (lat.get("p99") or 0) * 1000))
PY
stats() { python3 "$TMP/stats.py" "$1" stats; }
rps_of() { python3 "$TMP/stats.py" "$1" rps; }

echo "== spike_shed (max_inflight=32, replay 300ms, 200 conn ${DUR}s unique q) =="
read PID PORT < <(serve CAUCE_SERVER_MAX_INFLIGHT=32 CAUCE_REPLAY_LATENCY_MS=300)
PIDS="$PIDS $PID"
$oha --output-format json --output "$TMP/shed.json" -z ${DUR}s -c 200 --rand-regex-url "http://127.0.0.1:$PORT/api/search\\?q=[a-z]{8}"
read SHED_200 SHED_429 SHED_5XX SHED_P50 SHED_P99 < <(stats "$TMP/shed.json")
echo "SHED_200_PCT=$SHED_200 SHED_429_PCT=$SHED_429 SHED_5XX_PCT=$SHED_5XX SHED_P50_MS=$SHED_P50 SHED_P99_MS=$SHED_P99"
stop "$PID"

echo "== collapse (fail_every=2, latency 300ms, 50 identical cold q) =="
read PID PORT < <(serve CAUCE_REPLAY_LATENCY_MS=300 CAUCE_REPLAY_FAIL_EVERY=2)
PIDS="$PIDS $PID"
$oha --output-format json --output "$TMP/col.json" -n 50 -c 50 "http://127.0.0.1:$PORT/api/search?q=collapse-probe"
read COL_200 COL_429 COL_5XX COL_P50 COL_P99 < <(stats "$TMP/col.json")
echo "COLLAPSE_200_PCT=$COL_200 COLLAPSE_5XX_PCT=$COL_5XX COLLAPSE_P99_MS=$COL_P99 (5xx>0 means calls were NOT collapsed)"
stop "$PID"

echo "== constrained (taskset 1 CPU, latency 0, uncached ${DUR}s) =="
TASKSET_CMD=""; command -v taskset >/dev/null && TASKSET_CMD="taskset -c 0"
TASKSET_OK=0; [ -n "$TASKSET_CMD" ] && TASKSET_OK=1
read PID PORT < <(TASKSET_CMD="$TASKSET_CMD" serve CAUCE_REPLAY_LATENCY_MS=0)
PIDS="$PIDS $PID"
$oha --output-format json --output "$TMP/pin.json" -z ${DUR}s -c 20 --rand-regex-url "http://127.0.0.1:$PORT/api/search\\?q=[a-z]{10}"
CONST_RPS=$(rps_of "$TMP/pin.json")
sleep 0.3
CONST_RSS=$(awk '/VmHWM/{print int($2/1024)}' "/proc/$PID/status" 2>/dev/null || echo 0)
read PIN_200 PIN_429 PIN_5XX PIN_P50 PIN_P99 < <(stats "$TMP/pin.json")
echo "CONST_PINNED=$TASKSET_OK CONST_RPS=$CONST_RPS CONST_RSS_MB=$CONST_RSS CONST_P99_MS=$PIN_P99 CONST_5XX_PCT=$PIN_5XX"
stop "$PID"

echo "== edge_basis =="
read PID PORT < <(serve CAUCE_REPLAY_LATENCY_MS=200)
PIDS="$PIDS $PID"
COLD_MS=$(curl -s -o /dev/null -w '%{time_total}' "http://127.0.0.1:$PORT/api/search?q=edge" | python3 -c 'import sys;print(int(float(sys.stdin.read())*1000))')
$oha --output-format json --output "$TMP/edge.json" -n 100 -c 10 "http://127.0.0.1:$PORT/api/search?q=edge"
read E_200 E_429 E_5XX E_P50 E_P99 < <(stats "$TMP/edge.json")
HDR=$(curl -sI "http://127.0.0.1:$PORT/api/search?q=edge2" | tr -d '\r' | grep -i '^cache-control:' || echo none)
EDGE_SMAXAGE=missing; case "$HDR" in *s-maxage*|*max-age*) EDGE_SMAXAGE=present;; esac
echo "EDGE_SMAXAGE=$EDGE_SMAXAGE ($HDR) EDGE_COLD_MS=$COLD_MS EDGE_WARM_P50_MS=$E_P50 EDGE_WARM_P99_MS=$E_P99"
stop "$PID"
