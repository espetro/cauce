from typing import Any

from mcp.server.mcpserver import MCPServer

from . import exa_compat
from .cache import TTLCache

_cache: TTLCache | None = None


def set_cache(c: TTLCache) -> None:
    global _cache
    _cache = c


mcp = MCPServer(
    name="ex-search-proxy",
    instructions=(
        "Local Exa-compatible web search backed by DuckDuckGo with a TTL cache. "
        "Query returns Exa-shaped JSON: {requestId, searchType, results, costDollars} "
        "where each result has {title, url, id, text, highlights, highlightScores, "
        "publishedDate, author, image, favicon, extras}. Unimplemented Exa fields "
        "(deep search variants, contents.summary, additionalQueries, systemPrompt, "
        "outputSchema, stream) are silently ignored."
    ),
)


@mcp.tool(name="exa_search", description=(
    "Search the web via DuckDuckGo and return Exa-shaped JSON. "
    "Args: query (required), num_results (1-30, default 10), type ('auto'|'instant'; "
    "deep variants ignored), contents_highlights, contents_text, include_domains, "
    "exclude_domains, category ('news' for last 24h, else ''). "
    "Returns {requestId, searchType, results, costDollars, _source}."
))
def exa_search(
    query: str,
    num_results: int = 10,
    type: str = "auto",
    contents_highlights: bool = True,
    contents_text: bool = True,
    include_domains: list[str] | None = None,
    exclude_domains: list[str] | None = None,
    category: str = "",
) -> dict[str, Any]:
    req: dict[str, Any] = {
        "query": query,
        "numResults": num_results,
        "type": type,
        "contents": {"highlights": contents_highlights, "text": contents_text},
        "category": category,
    }
    if include_domains:
        req["includeDomains"] = include_domains
    if exclude_domains:
        req["excludeDomains"] = exclude_domains
    if _cache is None:
        return exa_compat.search(req)
    from .search import do_search
    out = do_search(_cache, req)
    return out
