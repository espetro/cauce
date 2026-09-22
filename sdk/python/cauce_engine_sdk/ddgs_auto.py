# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""ddgs reference engine: `DDGS().text(..., backend="auto")` over the cauce exec
protocol. The day-1 bridge for real results.

Run as `python3 sdk/python/cauce_engine_sdk/ddgs_auto.py` with the `ddgs` extra
installed (`uv sync --extra ddgs` in sdk/python).
"""
import sys
from pathlib import Path

if __package__ in (None, ""):
    # Started as a script: put sdk/python on sys.path so the package resolves.
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cauce_engine_sdk import Request, Response, Result, run

MAX_RESULTS = 10

# Protocol v2 params (issue #88) -> ddgs kwargs. ddgs safesearch is
# on|moderate|off; timelimit is d|w|m|y.
_SAFESEARCH = {"off": "off", "moderate": "moderate", "strict": "on"}
_TIMELIMIT = {"day": "d", "week": "w", "month": "m", "year": "y"}


def search(req: Request) -> Response:
    try:
        from ddgs import DDGS
    except ImportError:
        return Response(error="transport:ddgs not installed (uv sync --extra ddgs)")
    try:
        hits = DDGS().text(
            req.query,
            backend="auto",
            safesearch=_SAFESEARCH.get(req.safesearch, "moderate"),
            timelimit=_TIMELIMIT.get(req.time_range or ""),
            max_results=MAX_RESULTS,
            page=req.page,
            **req.params,
        )
    except Exception as exc:
        msg = str(exc).lower()
        if "ratelimit" in msg or "429" in msg or "too many" in msg:
            return Response(error="rate_limited")
        return Response(error=f"transport:{exc}")
    results = [
        Result(
            title=hit.get("title") or "",
            url=hit.get("href") or "",
            snippet=hit.get("body") or "",
        )
        for hit in hits or []
        if hit.get("href")
    ]
    # An empty page is `no_results`, not `Ok([])`: the pipeline still treats
    # it as an answer, but the report must not read as a healthy `Ok` (#120).
    if not results:
        return Response(error="no_results")
    return Response(results=results)


if __name__ == "__main__":
    run(search)
