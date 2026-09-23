# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""Tiny exec-engine fixture for the cauce-engines integration tests.

Answers every request with 3 synthetic results whose snippet carries the
child pid (`pid=<n>`) so tests can assert the process was respawned.

Flags:
  --crash-after N   os._exit(3) without answering when request N+1 arrives
                    (simulates a crash mid-call)
  --sleep S         sleep S seconds before answering a matching request
  --sleep-on STR    only sleep when STR is a substring of the query
  --boot-delay S    sleep S seconds once at startup, before reading requests
                    (simulates a slow cold start: interpreter + engine init)
"""
import argparse
import os
import sys
import time
from pathlib import Path

# crates/cauce-engines/tests/fixtures -> repo root -> sdk/python
sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "sdk" / "python"))

from cauce_engine_sdk import Request, Result, run  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--crash-after", type=int, default=0)
    ap.add_argument("--sleep", type=float, default=0.0)
    ap.add_argument("--sleep-on", default=None)
    ap.add_argument("--boot-delay", type=float, default=0.0)
    opts = ap.parse_args()
    if opts.boot_delay:
        time.sleep(opts.boot_delay)
    state = {"n": 0}

    def echo(req: Request):
        state["n"] += 1
        if opts.crash_after and state["n"] > opts.crash_after:
            os._exit(3)
        if opts.sleep and (opts.sleep_on is None or opts.sleep_on in req.query):
            time.sleep(opts.sleep)
        pid = os.getpid()
        return [
            Result(
                title=f"{req.query} result {i}",
                url=f"https://example.com/p{pid}/{i}",
                snippet=(
                    f"pid={pid} page={req.page} lang={req.lang} v={req.v}"
                    f" safesearch={req.safesearch} time_range={req.time_range}"
                    f" params={sorted(req.params.items())}"
                ),
            )
            for i in range(3)
        ]

    run(echo)


if __name__ == "__main__":
    main()
