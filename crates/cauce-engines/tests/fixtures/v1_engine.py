# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""Protocol-v1-only exec-engine fixture (issue #88 downgrade path).

Speaks exactly the settled v1 contract — no SDK import — so the parent must
negotiate down: a `v` other than 1 gets the reference SDK's v1 rejection
`parse:unsupported protocol version: <v>` and every other line is answered
as a v1 request. Answers 3 synthetic results whose snippet carries the
child pid, like echo_engine.py.
"""
import json
import os
import sys


def main() -> None:
    pid = os.getpid()
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            req = json.loads(line)
            v = req.get("v")
            if v != 1:
                raise ValueError(f"unsupported protocol version: {v!r}")
            # v1 children see the settled fields only; anything extra would
            # be ignored here, but the parent must omit v2 fields entirely.
            leaked = {"safesearch", "time_range", "params"} & set(req)
            query = req["query"]
            results = [
                {
                    "title": f"{query} result {i}",
                    "url": f"https://example.com/p{pid}/{i}",
                    "snippet": (
                        f"pid={pid} page={req.get('page', 1)}"
                        f" leaked_fields={sorted(leaked)}"
                    ),
                }
                for i in range(3)
            ]
            resp = {"v": 1, "results": results, "error": None}
        except Exception as exc:
            resp = {"v": 1, "results": [], "error": f"parse:{exc}"}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
