# Copyright (c) 2026 Joaquin Terrasa and oxe contributors. Apache-2.0.
"""ddgs reference engine: `DDGS().text(..., backend="auto")` over the oxe exec
protocol. The day-1 bridge for real results.

Run as `python3 sdk/python/oxe_engine_sdk/ddgs_auto.py` with the `ddgs` extra
installed (`uv sync --extra ddgs` in sdk/python).
"""
import sys
from pathlib import Path

if __package__ in (None, ""):
    # Started as a script: put sdk/python on sys.path so the package resolves.
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from oxe_engine_sdk import Request, Response, Result, run

MAX_RESULTS = 10


def search(req: Request) -> Response:
    try:
        from ddgs import DDGS
    except ImportError:
        return Response(error="transport:ddgs not installed (uv sync --extra ddgs)")
    try:
        hits = DDGS().text(
            req.query,
            backend="auto",
            max_results=MAX_RESULTS,
            page=req.page,
        )
    except Exception as exc:
        msg = str(exc).lower()
        if "ratelimit" in msg or "429" in msg or "too many" in msg:
            return Response(error="rate_limited")
        return Response(error=f"transport:{exc}")
    return Response(
        results=[
            Result(
                title=hit.get("title") or "",
                url=hit.get("href") or "",
                snippet=hit.get("body") or "",
            )
            for hit in hits or []
            if hit.get("href")
        ]
    )


if __name__ == "__main__":
    run(search)
