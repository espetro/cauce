"""Code-level smoke test for the MCP handshake: tools register and respond
with the right shape.

Per the task, this never touches a running/live MCP service -- it builds an
in-process ``MCPServer`` (``oxe.api.mcp.create_mcp_server``) over a tmp-path
``TTLCache`` and a stubbed ``SearchEngine`` (no network), lists its tools,
and calls each once. That's enough to prove the MCP wire shape (tool names,
argument schema, result shape) round-trips through the real ``mcp`` package's
tool registration and JSON-RPC-shaped ``call_tool`` path, without needing a
live search backend or a live MCP transport.
"""

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING, cast

from mcp_types import CallToolResult

from oxe.api.mcp import create_mcp_server
from oxe.cache import TTLCache
from oxe.jsontypes import JSONDict
from oxe.search.model import SearchRequest, SearchResult, SearxResponse
from oxe.search.service import SearchService

if TYPE_CHECKING:
    from mcp.server.mcpserver import MCPServer


class FakeEngine:
    """Matches the SearchEngine protocol; returns a canned response, no I/O."""

    def __init__(self) -> None:
        self.name = "fake"
        self.timeout = 10.0

    async def search(self, req: SearchRequest) -> SearxResponse:
        return SearxResponse(
            query=req.q,
            number_of_results=1,
            results=[
                SearchResult(url="https://example.com/a", title="A", content="hello", engine="fake")
            ],
        )


def _build_server(tmp_path: Path) -> "tuple[TTLCache, MCPServer]":
    cache = TTLCache(tmp_path / "db")
    service = SearchService(FakeEngine(), cache)
    server = create_mcp_server(service, cache)
    return cache, server


def _unwrap(structured_content: object) -> JSONDict:
    """``exa_search``'s return type is a union (web/cache vs. history shape);
    the ``mcp`` package wraps a union tool result's structured content under
    a top-level ``"result"`` key. ``exa_user_history`` has no union return, so
    its structured content is already the unwrapped body. The package types
    ``structured_content`` as ``Any`` (it is arbitrary JSON by design); this
    is the one point that boundary is narrowed into ``JSONDict``, same
    pattern as ``oxe.config``'s ``_as_object_dict``.
    """
    assert structured_content is not None
    body = cast(JSONDict, structured_content)
    if set(body) == {"result"}:
        return cast(JSONDict, body["result"])
    return body


def _call_tool(server: "MCPServer", name: str, arguments: JSONDict) -> CallToolResult:
    """``MCPServer.call_tool`` returns ``CallToolResult | InputRequiredResult``;
    the latter is only ever produced for elicitation flows, which none of
    these tools use, so this narrows the union for the tests below.
    """
    result = asyncio.run(server.call_tool(name, arguments))
    assert isinstance(result, CallToolResult)
    return result


def test_mcp_server_registers_both_tools(tmp_path: Path) -> None:
    _cache, server = _build_server(tmp_path)

    tools = asyncio.run(server.list_tools())

    assert {t.name for t in tools} == {"exa_search", "exa_user_history"}


def test_exa_search_web_source_returns_exa_shaped_result(tmp_path: Path) -> None:
    _cache, server = _build_server(tmp_path)

    result = _call_tool(server, "exa_search", {"query": "python"})
    body = _unwrap(result.structured_content)

    assert result.is_error is False
    assert body["searchType"] == "auto"
    assert body["tool_source"] == "web"
    results = cast(list[JSONDict], body["results"])
    assert results[0]["url"] == "https://example.com/a"
    assert "requestId" in body


def test_exa_search_history_source_short_circuits_to_clicks(tmp_path: Path) -> None:
    cache, server = _build_server(tmp_path)
    # get_clicks(query_text=...) joins clicks -> cache on query_hash for the
    # query text, so a click needs a matching cache row to be text-searchable.
    cache.set("hash1", {"_q": "clicked"}, ttl=3600)
    cache.record_click("hash1", "r1", "https://example.com/clicked", "Clicked", source="web-ui")

    result = _call_tool(server, "exa_search", {"query": "clicked", "source": "history"})
    body = _unwrap(result.structured_content)

    assert body["tool_source"] == "history"
    results = cast(list[JSONDict], body["results"])
    assert results[0]["url"] == "https://example.com/clicked"


def test_exa_user_history_returns_clicks_and_count(tmp_path: Path) -> None:
    cache, server = _build_server(tmp_path)
    cache.set("hash1", {"_q": "x"}, ttl=3600)
    cache.record_click("hash1", "r1", "https://example.com/x", "X", source="web-ui")

    result = _call_tool(server, "exa_user_history", {"limit": 5})
    body = _unwrap(result.structured_content)

    assert body["count"] == 1
    clicks = cast(list[JSONDict], body["clicks"])
    assert clicks[0]["url"] == "https://example.com/x"
