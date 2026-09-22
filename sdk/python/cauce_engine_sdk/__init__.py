# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""cauce exec engine SDK: protocol v1, one JSON object per line on stdio.

An engine is a `fn(Request) -> Iterable[Result] | Response` passed to `run()`.
Wire shapes (parent plan 4.3):

    -> {"v":1,"query":"...","page":1,"lang":"en","timeout_ms":1500}
    <- {"v":1,"results":[{"title":"...","url":"...","snippet":"..."}],"error":null}

Zero runtime dependencies; stdlib only.
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from typing import Callable, Iterable, Optional, Union

PROTOCOL_VERSION = 1


@dataclass
class Request:
    """One inbound search request line."""

    query: str
    page: int = 1
    lang: str = "en"
    timeout_ms: int = 1500

    @classmethod
    def from_json(cls, line: str) -> "Request":
        obj = json.loads(line)
        if not isinstance(obj, dict):
            raise ValueError("request is not a JSON object")
        if obj.get("v") != PROTOCOL_VERSION:
            raise ValueError(f"unsupported protocol version: {obj.get('v')!r}")
        query = obj.get("query")
        if not isinstance(query, str):
            raise ValueError("request missing string field 'query'")
        return cls(
            query=query,
            page=int(obj.get("page") or 1),
            lang=obj.get("lang") or "en",
            timeout_ms=int(obj.get("timeout_ms") or 1500),
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
            resp = Response(error=f"parse:{exc}")
        else:
            try:
                out = fn(req)
                resp = out if isinstance(out, Response) else Response(list(out))
            except Exception as exc:
                resp = Response(error=f"transport:{exc}")
        payload = {
            "v": PROTOCOL_VERSION,
            "results": [r.__dict__ for r in resp.results],
            "error": resp.error,
        }
        sys.stdout.write(json.dumps(payload) + "\n")
        sys.stdout.flush()


__all__ = ["PROTOCOL_VERSION", "Request", "Response", "Result", "run"]
