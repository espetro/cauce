"""``exa_search`` / ``exa_user_history`` MCP tools, on top of ``oxe.api.exa``.

Wire shape (tool names, argument names/defaults, description text, and the
``source='history'`` short-circuit) is kept unchanged from legacy
``oxe/mcp_server.py`` -- this is the surface the repo owner's global agent
wiring (Claude Code, Hermes, maki; see ``~/SEARCH.md``) already points at,
and the plan is explicit that no new capability is ever added to the Exa
shape. What changed from legacy: results are now typed pydantic models
instead of untyped dicts, and there is no module-level mutable ``_cache``
global -- ``create_mcp_server`` takes ``SearchService``/``TTLCache`` and
closes over them, so a test (or a future multi-process host) can build an
independent server per cache/service pair.

``mcp>=2.2`` renamed ``FastMCP`` to ``mcp.server.mcpserver.MCPServer``; the
decorator/registration API legacy used (``@mcp.tool(name=..., description=
...)``) is unchanged in substance across that rename.
"""

from typing import TYPE_CHECKING

from pydantic import BaseModel, ConfigDict, Field

from oxe.api.exa import (
    ExaContents,
    ExaCostDollars,
    ExaResultItem,
    ExaSearchRequest,
    exa_request_to_query,
    searx_response_to_exa,
)
from oxe.cache import ClickRow, SearchLogMeta, TTLCache
from oxe.search.service import SearchService, cache_key

if TYPE_CHECKING:
    from mcp.server.mcpserver import MCPServer

_MCP_INSTRUCTIONS = (
    "Local Exa-compatible web search backed by DuckDuckGo with a TTL cache. "
    "Query returns Exa-shaped JSON: {requestId, searchType, results, costDollars} "
    "where each result has {title, url, id, text, highlights, highlightScores, "
    "publishedDate, author, image, favicon, extras}. Unimplemented Exa fields "
    "(deep search variants, contents.summary, additionalQueries, systemPrompt, "
    "outputSchema, stream) are silently ignored."
)

_EXA_SEARCH_DESCRIPTION = (
    "Search the web via DuckDuckGo and return Exa-shaped JSON. "
    "Args: query (required), num_results (1-30, default 10), type ('auto'|'instant'; "
    "deep variants ignored), source ('web'|'history'|'cache'; default 'web'), "
    "exclude_domains, category ('news' for last 24h, else ''). "
    "Returns {requestId, searchType, results, costDollars, tool_source}."
)

_EXA_USER_HISTORY_DESCRIPTION = (
    "Recent URLs the user has clicked from the search UI for a given query. "
    "Use this to avoid re-researching what the user has already explored. "
    "Args: query (optional substring match against query text), query_hash "
    "(optional exact match), limit (1-200, default 20), since_hours (default 168 = 1 week). "
    "Returns {clicks: [{query_hash, query, result_id, url, title, clicked_at, source}], count}."
)

_VALID_SOURCES = ("web", "history", "cache")
_SINCE_HOURS_BOUNDS = (1, 24 * 365)
_HISTORY_LIMIT_BOUNDS = (1, 200)


class McpExaSearchResult(BaseModel):
    """``exa_search``'s ``source in ('web', 'cache')`` result shape.

    Same field set as ``oxe.api.exa.ExaSearchResponse`` plus ``tool_source``
    (an oxe-specific field legacy attached ad hoc via its ``extra="allow"``
    response model; not part of Exa's own published schema, so it lives here
    rather than on the shared, frozen ``ExaSearchResponse``).
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, populate_by_name=True)

    request_id: str = Field(alias="requestId")
    search_type: str = Field(alias="searchType")
    results: list[ExaResultItem]
    cost_dollars: ExaCostDollars = Field(alias="costDollars")
    tool_source: str


class McpHistoryClick(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    id: int
    query_hash: str
    query: str
    result_id: str
    url: str
    title: str
    clicked_at: int
    source: str


class McpExaHistoryResult(BaseModel):
    """``exa_search``'s ``source='history'`` result shape (unchanged from legacy)."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    query: str
    tool_source: str = "history"
    results: list[McpHistoryClick]


class McpUserHistoryResult(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    clicks: list[McpHistoryClick]
    count: int


def _click_to_wire(row: ClickRow) -> McpHistoryClick:
    return McpHistoryClick(
        id=row.id,
        query_hash=row.query_hash,
        query=row.query,
        result_id=row.result_id,
        url=row.url,
        title=row.title,
        clicked_at=row.clicked_at,
        source=row.source,
    )


def create_mcp_server(service: SearchService, cache: TTLCache) -> "MCPServer":
    """Builds the ``oxe`` MCP server with ``exa_search``/``exa_user_history`` bound
    to the given service/cache. Import of ``MCPServer`` is local to this function
    (not the module top level) so importing ``oxe.api.mcp`` for its pydantic wire
    models alone never requires the ``mcp`` package's transport stack.
    """
    from mcp.server.mcpserver import MCPServer  # noqa: PLC0415

    server = MCPServer(name="oxe", instructions=_MCP_INSTRUCTIONS)

    # Keyword-only past `query`: the MCP client always calls tools with named
    # arguments (they come off a JSON object, not a positional list), so this
    # is the exact legacy wire shape while also keeping every arg out of
    # ruff's positional-argument counting (PLR0917) and its boolean-
    # positional-trap check (FBT001/FBT002) -- both of which read "positional"
    # from the python signature, not from how MCP actually calls the function.
    # PLR0913's *total* arg count (9) still trips the repo's max-args=6 gate;
    # that count is dictated by the frozen legacy MCP wire shape this
    # function preserves verbatim (see the module docstring), not by a
    # grouping choice made here, so it is a targeted, documented exception
    # rather than a case for restructuring the signature.
    @server.tool(name="exa_search", description=_EXA_SEARCH_DESCRIPTION)
    async def exa_search(  # noqa: PLR0913
        query: str,
        *,
        num_results: int = 10,
        type: str = "auto",
        contents_highlights: bool = True,
        contents_text: bool = True,
        include_domains: list[str] | None = None,
        exclude_domains: list[str] | None = None,
        category: str = "",
        source: str = "web",
    ) -> McpExaSearchResult | McpExaHistoryResult:
        resolved_source = source if source in _VALID_SOURCES else "web"
        if resolved_source == "history":
            rows = cache.get_clicks(query_text=query, limit=num_results)
            return McpExaHistoryResult(query=query, results=[_click_to_wire(r) for r in rows])

        req = ExaSearchRequest(
            query=query,
            type=type,
            numResults=num_results,
            category=category,
            includeDomains=include_domains or [],
            excludeDomains=exclude_domains or [],
            contents=ExaContents(text=contents_text, highlights=contents_highlights),
        )
        search_req = exa_request_to_query(req)
        response = await service.search(search_req)
        exa_response = searx_response_to_exa(response, req)
        cache.log_search(
            query_text=query[:200],
            query_hash=cache_key(search_req),
            source=resolved_source,
            meta=SearchLogMeta(backend="ddg", result_count=len(exa_response.results), client="mcp"),
        )
        return McpExaSearchResult(
            requestId=exa_response.request_id,
            searchType=exa_response.search_type,
            results=exa_response.results,
            costDollars=exa_response.cost_dollars,
            tool_source=resolved_source,
        )

    @server.tool(name="exa_user_history", description=_EXA_USER_HISTORY_DESCRIPTION)
    async def exa_user_history(
        query: str = "",
        query_hash: str = "",
        limit: int = 20,
        since_hours: int = 168,
    ) -> McpUserHistoryResult:
        qh = query_hash.strip() or None
        qt = query.strip() or None
        since = max(_SINCE_HOURS_BOUNDS[0], min(since_hours, _SINCE_HOURS_BOUNDS[1]))
        lim = max(_HISTORY_LIMIT_BOUNDS[0], min(limit, _HISTORY_LIMIT_BOUNDS[1]))
        rows = cache.get_clicks(query_hash=qh, query_text=qt, limit=lim, since_hours=since)
        clicks = [_click_to_wire(r) for r in rows]
        return McpUserHistoryResult(clicks=clicks, count=len(clicks))

    return server
