# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""cauce exec engine SDK: protocol v2, one JSON object per line on stdio.

An engine is a `fn(Request) -> Iterable[Result] | Response` passed to `run()`.
Wire shapes (parent plan 4.3, issue #88):

    -> {"v":2,"query":"...","page":1,"lang":"en","timeout_ms":1500,
        "safesearch":"moderate","time_range":"week","params":{"k":"v"}}
    <- {"v":2,"results":[{"title":"...","url":"...","snippet":"..."}],"error":null}

Version negotiation: the parent speaks v2 optimistically and downgrades a
child that rejects it, so engines built against this SDK must also accept
`v:1` requests (a strict subset: no safesearch/time_range/params). Responses
echo the request's `v`, which keeps old v1 parents working against new
children.

Zero runtime dependencies; stdlib only.
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from typing import Callable, Iterable, Optional, Union

PROTOCOL_VERSION = 2
MIN_PROTOCOL_VERSION = 1


@dataclass
class Request:
    """One inbound search request line.

    `safesearch`/`time_range`/`params` only arrive on v2 requests; on v1 they
    hold the defaults below. `v` is the negotiated protocol version of this
    request line, echoed back in the response.
    """

    query: str
    page: int = 1
    lang: str = "en"
    timeout_ms: int = 1500
    safesearch: str = "moderate"
    time_range: Optional[str] = None
    params: dict = field(default_factory=dict)
    v: int = MIN_PROTOCOL_VERSION

    @classmethod
    def from_json(cls, line: str) -> "Request":
        obj = json.loads(line)
        if not isinstance(obj, dict):
            raise ValueError("request is not a JSON object")
        v = obj.get("v")
        if not isinstance(v, int) or not MIN_PROTOCOL_VERSION <= v <= PROTOCOL_VERSION:
            raise ValueError(f"unsupported protocol version: {v!r}")
        query = obj.get("query")
        if not isinstance(query, str):
            raise ValueError("request missing string field 'query'")
        params = obj.get("params")
        return cls(
            query=query,
            page=int(obj.get("page") or 1),
            lang=obj.get("lang") or "en",
            timeout_ms=int(obj.get("timeout_ms") or 1500),
            safesearch=obj.get("safesearch") or "moderate",
            time_range=obj.get("time_range"),
            params=params if isinstance(params, dict) else {},
            v=v,
        )


@dataclass
class Result:
    """One organic result row."""

    title: str
    url: str
    snippet: str = ""


@dataclass
class Response:
    """One outbound response line; `error` is a short snake_case code
    (`rate_limited`, `blocked`, `no_results`, `parse:<msg>`,
    `transport:<msg>`)."""

    results: list[Result] = field(default_factory=list)
    error: Optional[str] = None


def run(fn: Callable[[Request], Union[Iterable[Result], Response]]) -> None:
    """Serve requests from stdin until EOF, one response line each.

    Malformed lines and handler exceptions produce an error response instead
    of crashing; EOF exits cleanly.
    """
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            req = Request.from_json(line)
        except Exception as exc:
            req = None
            resp = Response(error=f"parse:{exc}")
        else:
            try:
                out = fn(req)
                resp = out if isinstance(out, Response) else Response(list(out))
            except Exception as exc:
                resp = Response(error=f"transport:{exc}")
        # Echo the request's protocol version so old parents keep working
        # against this child and new parents can confirm the negotiated level.
        payload = {
            "v": req.v if req is not None else PROTOCOL_VERSION,
            "results": [r.__dict__ for r in resp.results],
            "error": resp.error,
        }
        sys.stdout.write(json.dumps(payload) + "\n")
        sys.stdout.flush()


__all__ = [
    "MIN_PROTOCOL_VERSION",
    "PROTOCOL_VERSION",
    "Request",
    "Response",
    "Result",
    "run",
]
