"""AI answer pipeline: tool loop over an OpenAI-compatible chat endpoint.

Wave 4 rewrite of legacy ``oxe/ai.py`` (642 lines, two parse defects and a
silently-disabled ``tools`` kwarg logged by the retro). Structure changes:

- Frames are the typed union from ``oxe/api/ai_frames.py`` (``StepFrame``,
  ``DeltaFrame``, ``SourcesFrame``, ``DoneFrame``, ``ErrorFrame``), not
  loose event dicts.
- The provider client is stdlib ``urllib`` (no aisuite/openai/anthropic
  dependency) against the OpenAI-compatible ``/chat/completions`` endpoint
  every configured provider exposes (openai, groq, mistral, ollama,
  huggingface, and anthropic's OpenAI-compat layer). ``TOOLS_SPEC`` is sent
  on every request; ``tests/api/test_answer.py`` asserts it so the kwarg
  cannot silently disable itself again (plan step 20).
- ``parse_final_answer`` fixes both legacy defects: the trailing JSON tail
  is found even when pretty-printed across multiple lines (legacy only
  scanned the last 4 single lines for a same-line ``confidence`` +
  ``related_questions`` pair), and a garbled tail no longer leaks raw JSON
  into the returned answer body.

Loop constants (``MAX_ITERATIONS``, ``CACHE_MIN_CONFIDENCE``) live in
``oxe.config`` so the caps are configuration facts, not magic numbers here.
"""

import asyncio
import hashlib
import json
import logging
import urllib.error
import urllib.request
from collections.abc import AsyncGenerator, Awaitable, Callable, Mapping
from dataclasses import dataclass
from typing import cast

from oxe.api.ai_frames import (
    AnswerFrame,
    AnswerSource,
    DeltaFrame,
    DoneFrame,
    SourcesFrame,
    StepFrame,
)
from oxe.cache import TTLCache
from oxe.config import MAX_ITERATIONS, AIConfig
from oxe.errors import ProviderError
from oxe.jsontypes import JSONDict, JSONValue
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest
from oxe.search.service import SearchService

log = logging.getLogger(__name__)

CONFIDENCE_THRESHOLD = 8
PROVIDER_TIMEOUT_S = 60.0

# Per-provider OpenAI-compatible base URLs; ``AIConfig.base_url`` overrides.
DEFAULT_BASE_URLS: dict[str, str] = {
    "openai": "https://api.openai.com/v1",
    "anthropic": "https://api.anthropic.com/v1",
    "groq": "https://api.groq.com/openai/v1",
    "mistral": "https://api.mistral.ai/v1",
    "ollama": "http://localhost:11434/v1",
    "huggingface": "https://router.huggingface.co/v1",
}

SYSTEM_PROMPT = (
    "You are the answer engine for oxe, a local web-search proxy. Answer the "
    "user's query using the provided tools when you need fresh information. "
    "Be concise and factual. Cite sources inline as [n] matching the URLs you "
    "used from tool results.\n"
    "When you are done answering, you MUST end your final message with a "
    "single JSON object exactly like:\n"
    '{"confidence": <1-10>, "related_questions": ["q1", "q2", "q3"]}\n'
    "confidence expresses how sure you are in the answer (use >= 8 only when "
    "well supported by sources)."
)

# Sent as the request's ``tools`` kwarg on EVERY provider call; the presence
# of this key on the wire is asserted by test so it cannot be dropped again.
TOOLS_SPEC: list[JSONDict] = [
    {
        "type": "function",
        "function": {
            "name": "web_search",
            "description": (
                "Search the web (or local click history) and return results "
                "with title, url and text snippets."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "source": {
                        "type": "string",
                        "enum": ["web", "history"],
                        "description": "web = live search, history = user's clicked URLs",
                    },
                    "num_results": {"type": "integer", "minimum": 1, "maximum": 10},
                },
                "required": ["query"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "user_history",
            "description": (
                "URLs the user recently clicked in the search UI for a query. "
                "Use to avoid re-researching what the user already explored."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                },
            },
        },
    },
]

ToolFn = Callable[..., Awaitable[JSONDict]]


def answer_cache_key(query: str, model: str) -> str:
    """Cache identity for a final answer: normalized query text + model."""
    norm = ((query or "").lower().strip(), model)
    return hashlib.sha256(repr(norm).encode("utf-8")).hexdigest()


def parse_final_answer(text: str) -> tuple[str, int, list[str]]:
    """Split the trailing JSON tail into ``(clean_answer, confidence, related)``.

    Tolerant: a missing or garbled tail yields the whole text with
    confidence 0. Unlike legacy, the tail may be pretty-printed across
    multiple lines, and on a garbled tail the raw JSON never leaks into the
    answer body (legacy defect: it returned the unparsed tail as the
    answer when its 4-line same-line scan missed).
    """
    stripped = text.rstrip()
    window_start = max(0, len(stripped) - 500)
    brace_indices = [i for i, ch in enumerate(stripped) if ch == "{" and i >= window_start]
    for idx in reversed(brace_indices):
        try:
            parsed: JSONValue = json.loads(stripped[idx:])
        except json.JSONDecodeError:
            continue
        if not isinstance(parsed, dict) or "confidence" not in parsed:
            continue
        try:
            conf = int(str(parsed["confidence"]))
        except (TypeError, ValueError):
            continue
        related_raw: JSONValue = parsed.get("related_questions", [])
        related_list: list[JSONValue] = list(related_raw) if isinstance(related_raw, list) else []
        related = [str(q)[:200] for q in related_list[:5]]
        return stripped[:idx].rstrip(), max(0, min(10, conf)), related
    return stripped, 0, []


@dataclass(frozen=True)
class ToolCall:
    """One tool invocation requested by the model."""

    id: str
    name: str
    query: str
    arguments: str


@dataclass(frozen=True)
class ProviderReply:
    """The model's reply for one loop iteration."""

    text: str
    tool_calls: tuple[ToolCall, ...]


def _endpoint(cfg: AIConfig) -> str:
    base = cfg.base_url or DEFAULT_BASE_URLS.get(cfg.provider, DEFAULT_BASE_URLS["openai"])
    return base.rstrip("/") + "/chat/completions"


def _narrow_body(raw: bytes) -> JSONDict:
    parsed: JSONValue = json.loads(raw)
    return parsed if isinstance(parsed, dict) else {}


def _tool_calls_of(message: JSONDict) -> tuple[ToolCall, ...]:
    raw_calls = message.get("tool_calls")
    if not isinstance(raw_calls, list):
        return ()
    calls: list[ToolCall] = []
    for rc in raw_calls:
        if not isinstance(rc, dict):
            continue
        fn = rc.get("function")
        if not isinstance(fn, dict):
            continue
        name = str(fn.get("name") or "")
        arguments = str(fn.get("arguments") or "{}")
        query = ""
        try:
            args: JSONValue = json.loads(arguments)
        except json.JSONDecodeError:
            args = None
        if isinstance(args, dict) and args.get("query"):
            query = str(args["query"])
        calls.append(
            ToolCall(id=str(rc.get("id") or ""), name=name, query=query, arguments=arguments)
        )
    return tuple(calls)


def _chat_completion(cfg: AIConfig, messages: list[JSONDict]) -> ProviderReply:
    """One blocking, non-streaming chat completion (runs via ``to_thread``).

    ``tools`` is always sent: legacy's aisuite client dropped the kwarg
    unless ``max_turns`` was set, silently disabling tool calling.
    """
    payload: JSONDict = {
        "model": cfg.model,
        "messages": cast(JSONValue, messages),
        "tools": cast(JSONValue, TOOLS_SPEC),
    }
    headers = {"Content-Type": "application/json"}
    api_key = cfg.resolve_api_key()
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    request = urllib.request.Request(  # noqa: S310 - https/http only, from AIConfig
        _endpoint(cfg),
        data=json.dumps(payload).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        # The endpoint is built only from AIConfig (base_url / the provider
        # allowlist in oxe.config), never from raw user input.
        with urllib.request.urlopen(request, timeout=PROVIDER_TIMEOUT_S) as resp:  # noqa: S310
            body = _narrow_body(resp.read())
    except urllib.error.HTTPError as e:
        raise ProviderError(_friendly_provider_error(e, cfg)) from e
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, OSError) as e:
        raise ProviderError(_friendly_provider_error(e, cfg)) from e

    choices = body.get("choices")
    if not isinstance(choices, list) or not choices or not isinstance(choices[0], dict):
        return ProviderReply(text="", tool_calls=())
    message = choices[0].get("message")
    if not isinstance(message, dict):
        return ProviderReply(text="", tool_calls=())
    content = message.get("content")
    return ProviderReply(
        text=content if isinstance(content, str) else "",
        tool_calls=_tool_calls_of(message),
    )


def _friendly_provider_error(e: Exception, cfg: AIConfig) -> str:
    """Translate provider exceptions into short, actionable user messages."""
    s = str(e)
    low = s.lower()
    if "401" in s or "unauthorized" in low or "invalid api key" in low:
        return "401 unauthorized - check the api key in settings"
    if "429" in s or "rate" in low:
        return "rate limited (429) by the provider - try again shortly"
    if "404" in s:
        return f"model '{cfg.model}' or endpoint not found - check settings"
    if "timed out" in low or "timeout" in low:
        return "provider request timed out"
    return f"provider error: {s[:200]}"


async def _run_web_search(
    service: SearchService, cache: TTLCache, query: str, source: str, num_results: int
) -> JSONDict:
    limit = max(1, min(10, num_results))
    if source == "history":
        rows = await asyncio.to_thread(cache.get_clicks, query_text=query, limit=limit)
        results: list[JSONValue] = [{"title": r.title, "url": r.url, "text": ""} for r in rows]
        return {"query": query, "source": "history", "results": results}
    resp = await service.search(SearchRequest(q=query))
    results = [
        {"title": r.title, "url": r.url, "text": r.content[:800]} for r in resp.results[:limit]
    ]
    return {"query": query, "source": "web", "results": results}


async def _run_user_history(cache: TTLCache, query: str, limit: int) -> JSONDict:
    rows = await asyncio.to_thread(
        cache.get_clicks, query_text=query or None, limit=max(1, min(50, limit))
    )
    clicks: list[JSONValue] = [{"url": r.url, "title": r.title, "query": r.query} for r in rows]
    return {"clicks": clicks, "count": len(clicks)}


def build_toolset(service: SearchService, cache: TTLCache) -> dict[str, ToolFn]:
    """Tool name -> async callable, mirroring the MCP tools.

    Returns partial-applied closures over the app-scoped service/cache; the
    model-facing arguments arrive per-call in ``stream_answer``'s dispatch.
    """

    async def web_search(query: str = "", source: str = "web", num_results: int = 5) -> JSONDict:
        return await _run_web_search(service, cache, query, source, num_results)

    async def user_history(query: str = "", limit: int = 20) -> JSONDict:
        return await _run_user_history(cache, query, limit)

    return {"web_search": web_search, "user_history": user_history}


async def _execute_tool(tools: Mapping[str, ToolFn], call: ToolCall) -> JSONDict:
    fn = tools.get(call.name)
    if fn is None:
        return {"error": f"unknown tool {call.name}"}
    try:
        args: JSONValue = json.loads(call.arguments or "{}")
    except json.JSONDecodeError:
        args = None
    kwargs: JSONDict = args if isinstance(args, dict) else {}
    try:
        if call.name == "web_search":
            query_raw = kwargs.get("query")
            source_raw = kwargs.get("source")
            n_raw = kwargs.get("num_results")
            return await fn(
                query=query_raw if isinstance(query_raw, str) else "",
                source=source_raw if isinstance(source_raw, str) else "web",
                num_results=n_raw if isinstance(n_raw, int) else 5,
            )
        limit_raw = kwargs.get("limit")
        return await fn(
            query=kwargs.get("query") if isinstance(kwargs.get("query"), str) else "",
            limit=limit_raw if isinstance(limit_raw, int) else 20,
        )
    except (BackendError, ValueError, TypeError) as e:
        log.warning("tool %s failed: %s", call.name, e)
        return {"error": str(e)}


def _collect_sources(result: JSONDict, seen: set[str], sources: list[AnswerSource]) -> None:
    raw_results = result.get("results")
    if not isinstance(raw_results, list):
        return
    for r in raw_results:
        if not isinstance(r, dict):
            continue
        url = r.get("url")
        if not isinstance(url, str) or not url or url in seen:
            continue
        seen.add(url)
        title = r.get("title")
        sources.append(
            AnswerSource(title=title if isinstance(title, str) and title else url, url=url)
        )


def _assistant_message(reply: ProviderReply) -> JSONDict:
    message: JSONDict = {"role": "assistant", "content": reply.text or None}
    if reply.tool_calls:
        message["tool_calls"] = [
            {
                "id": tc.id,
                "type": "function",
                "function": {"name": tc.name, "arguments": tc.arguments},
            }
            for tc in reply.tool_calls
        ]
    return message


async def stream_answer(
    query: str,
    cfg: AIConfig,
    tools: Mapping[str, ToolFn],
    *,
    max_iterations: int = MAX_ITERATIONS,
) -> AsyncGenerator[AnswerFrame, None]:
    """Run the ReAct tool loop, yielding typed SSE frames.

    The blocking provider call runs behind ``asyncio.to_thread`` (ASYNC
    rule); tool execution is async (search service) or threaded (sqlite).
    """
    model_id = f"{cfg.provider}:{cfg.model}"
    messages: list[JSONDict] = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": query},
    ]
    seen_urls: set[str] = set()
    sources: list[AnswerSource] = []

    for _iteration in range(max_iterations):
        try:
            reply = await asyncio.to_thread(_chat_completion, cfg, messages)
        except ProviderError as e:
            yield DoneFrame(answer="", confidence=0, model=model_id, cached=False, error=str(e))
            return

        if not reply.tool_calls:
            answer, confidence, related = parse_final_answer(reply.text)
            yield DeltaFrame(text=reply.text)
            yield SourcesFrame(sources=sources)
            yield DoneFrame(
                answer=answer,
                related_questions=related,
                confidence=confidence,
                model=model_id,
                cached=False,
            )
            return

        messages.append(_assistant_message(reply))
        for call in reply.tool_calls:
            yield StepFrame(
                tool=call.name,
                query=call.query,
                label=f"Searching: {call.query or call.name}",
            )
            result = await _execute_tool(tools, call)
            _collect_sources(result, seen_urls, sources)
            messages.append(
                {
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": json.dumps(result, default=str),
                }
            )

    yield DoneFrame(
        answer="",
        confidence=0,
        model=model_id,
        cached=False,
        error=f"exceeded max iterations ({max_iterations}) without a final answer",
    )
