from typing import Any
import logging

from mcp.server.mcpserver import MCPServer

from . import exa_compat
from .cache import TTLCache

log = logging.getLogger(__name__)

_cache: TTLCache | None = None


def set_cache(c: TTLCache) -> None:
    global _cache
    _cache = c


mcp = MCPServer(name="oxe", version="0.2.0")


@mcp.tool(description="Search the web via DuckDuckGo and return Exa-shaped JSON. Args: query (required), num_results (1-30, default 10), type ('auto'|'instant'; deep variants ignored), contents_highlights, contents_text, include_domains, exclude_domains, category ('news' for last 24h, else ''). Returns {requestId, searchType, results, costDollars, _source}.")
def exa_search(
    query: str,
    num_results: int = 10,
    type: str = "auto",
    contents_highlights: bool = True,
    contents_text: bool = True,
    include_domains: list[str] | None = None,
    exclude_domains: list[str] | None = None,
    category: str = "",
) -> dict:
    req: dict[str, Any] = {
        "query": query,
        "numResults": num_results,
        "type": type,
        "contents": {"highlights": contents_highlights, "text": contents_text},
    }
    if include_domains:
        req["includeDomains"] = include_domains
    if exclude_domains:
        req["excludeDomains"] = exclude_domains
    if category:
        req["category"] = category
    if _cache is None:
        return exa_compat.search(req)
    from .search import do_search
    out, duration_ms = do_search(_cache, req, with_duration=True)
    try:
        _cache.log_search(
            query_text=(query or "")[:200],
            query_hash=out.get("_q_hash") or "",
            source=out.get("_source") or "network",
            result_count=len(out.get("results") or []),
            duration_ms=duration_ms,
            client="mcp",
        )
    except Exception:
        log.exception("mcp: failed to write search_log row")
    return dict(out)


@mcp.tool(name="exa_user_history", description=(
    "Recent URLs the user clicked from the oxe web UI, filtered by query text or hash."
    " Use BEFORE searching to reuse what the user already explored."
))
def exa_user_history(
    query: str = "",
    query_hash: str = "",
    limit: int = 20,
    since_hours: int = 168,
) -> dict:
    if _cache is None:
        return {"clicks": [], "count": 0, "error": "cache not initialized"}
    qh = query_hash.strip() or None
    qt = query.strip() or None
    since = max(1, min(since_hours, 24 * 365))
    lim = max(1, min(limit, 200))
    rows = _cache.get_clicks(query_hash=qh, query_text=qt, limit=lim, since_hours=since)
    return dict(clicks=rows, count=len(rows))
