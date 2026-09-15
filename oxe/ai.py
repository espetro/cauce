"""AI answer pipeline: manual streaming tool loop over aisuite (lazy import).

All aisuite/provider SDK imports are lazy (inside functions) so the base
install never pulls them in. Emitted SSE event dicts:

  {"type": "step",    "tool": "web_search", "query": "...", "label": "Searching..."}
  {"type": "delta",   "text": "..."}
  {"type": "sources", "sources": [{title, url, favicon?}]}
  {"type": "done",    "answer", "related_questions", "confidence", "model",
                      "cached": bool}
"""

import hashlib
import json
import logging
import re
from collections.abc import AsyncGenerator, Callable

from .config import AIConfig

log = logging.getLogger(__name__)

MAX_ITERATIONS = 5
CONFIDENCE_THRESHOLD = 8
ANSWER_TTL_DEFAULT = 86400
ANSWER_TTL_MAX = 7 * 86400

CONFIDENCE_RE = re.compile(r'"confidence"\s*:\s*(\d+)')

SYSTEM_PROMPT = (
    "You are the answer engine for oxe, a local web-search proxy. Answer the "
    "user's query using the provided tools when you need fresh information. "
    "Be concise and factual. Cite sources inline as [n] matching the URLs you "
    "used from tool results.\n"
    "When you are done answering, you MUST end your final message with a "
    "single JSON line exactly like:\n"
    '{"confidence": <1-10>, "related_questions": ["q1", "q2", "q3"]}\n'
    "confidence expresses how sure you are in the answer (use >= 8 only when "
    "well supported by sources)."
)

TOOLS_SPEC = [
    {
        "type": "function",
        "function": {
            "name": "web_search",
            "description": (
                "Search the web (or local click history / cache) and return "
                "Exa-shaped results with title, url and text snippets."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "source": {
                        "type": "string",
                        "enum": ["web", "history", "cache"],
                        "description": "web = live search, history = user's clicked URLs, cache = previously cached searches",
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


def answer_cache_key(query: str, model: str) -> str:
    norm = (query or "").lower().strip(), model
    return hashlib.sha256(repr(norm).encode("utf-8")).hexdigest()


def _make_search_tool(cache, backend, on_result) -> Callable:
    from .search import do_search

    def web_search(query: str, source: str = "web", num_results: int = 5) -> dict:
        source = source if source in ("web", "history", "cache") else "web"
        num_results = max(1, min(10, int(num_results)))
        req = {
            "query": query,
            "numResults": num_results,
            "contents": {"text": True, "highlights": True},
        }
        if source == "history":
            rows = cache.get_clicks(query_text=query, limit=num_results) if cache else []
            return {"query": query, "source": "history", "results": rows}
        out = do_search(cache, req, backend=backend, on_result=on_result)
        return {
            "query": query,
            "source": source if source == "cache" else (out.get("_source") or "network"),
            "results": [
                {"title": r.get("title"), "url": r.get("url"), "text": (r.get("text") or "")[:800]}
                for r in (out.get("results") or [])
            ],
        }

    return web_search


def _make_history_tool(cache) -> Callable:
    def user_history(query: str = "", limit: int = 20) -> dict:
        if cache is None:
            return {"clicks": [], "count": 0}
        rows = cache.get_clicks(query_text=query or None, limit=max(1, min(50, int(limit))))
        return {"clicks": rows, "count": len(rows)}

    return user_history


def build_toolset(cache, backend, on_result) -> dict[str, Callable]:
    """Tool name -> python callable, mirroring the MCP tools."""
    return {
        "web_search": _make_search_tool(cache, backend, on_result),
        "user_history": _make_history_tool(cache),
    }


def _aisuite_client(cfg: AIConfig):
    """Lazy aisuite import; configured per request from the config file."""
    import aisuite as aisuite_mod  # lazy: only with oxe[ai]

    kwargs = {}
    api_key = cfg.resolve_api_key()
    if api_key:
        kwargs["api_key"] = api_key
    if cfg.base_url:
        kwargs["base_url"] = cfg.base_url
    provider = cfg.provider
    client = aisuite_mod.Client({provider: kwargs} if kwargs else {})
    return client, f"{provider}:{cfg.model}"


def parse_final_answer(text: str) -> tuple[str, int, list[str]]:
    """Split the trailing JSON line into (clean_answer, confidence, related).

    Tolerant: missing/garbled JSON yields the whole text with confidence 0
    (loop keeps iterating / low-confidence done event).
    """
    lines = [l for l in text.rstrip().splitlines() if l.strip()]
    conf, related = 0, []
    body = text
    for i in range(len(lines) - 1, max(len(lines) - 4, -1), -1):
        m = CONFIDENCE_RE.search(lines[i])
        if m and '"related_questions"' in lines[i]:
            try:
                parsed = json.loads(lines[i])
                conf = int(parsed.get("confidence") or 0)
                rq = parsed.get("related_questions")
                if isinstance(rq, list):
                    related = [str(q)[:200] for q in rq[:5]]
            except (json.JSONDecodeError, ValueError):
                conf = int(m.group(1))
            body = "\n".join(lines[:i]).rstrip()
            break
    return body, max(0, min(10, conf)), related


def _sources_from_messages(messages: list[dict]) -> list[dict]:
    """Collect deduplicated sources from tool results in the transcript."""
    seen: set[str] = set()
    sources: list[dict] = []
    for msg in messages:
        if msg.get("role") != "tool":
            continue
        try:
            data = json.loads(msg.get("content") or "{}")
        except (json.JSONDecodeError, TypeError):
            continue
        for r in data.get("results") or []:
            url = r.get("url")
            if not url or url in seen:
                continue
            seen.add(url)
            sources.append({"title": r.get("title") or url, "url": url})
    return sources[:20]


async def stream_answer(
    query: str,
    cfg: AIConfig,
    cache=None,
    backend=None,
    on_result=None,
    max_iterations: int = MAX_ITERATIONS,
) -> AsyncGenerator[dict, None]:
    """Run the ReAct tool loop, yielding SSE event dicts.

    Tool execution is sync (DDG + sqlite); the LLM streaming call runs in a
    worker thread so the event loop is never blocked.
    """
    import asyncio

    tools = build_toolset(cache, backend, on_result)
    messages: list[dict] = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": query},
    ]

    client, model_id = await asyncio.to_thread(_aisuite_client, cfg)

    for _iteration in range(max_iterations):
        collected: list[str] = []
        acc: list[dict] = []
        try:
            stream = await asyncio.to_thread(
                client.chat.completions.create,
                model=model_id,
                messages=messages,
                tools=TOOLS_SPEC,
                stream=True,
            )
        except Exception as e:  # noqa: BLE001 - surfaced as an error event
            yield {"type": "done", "answer": "", "related_questions": [],
                   "confidence": 0, "model": model_id, "cached": False,
                   "error": f"provider error: {e}"}
            return
        # aisuite streaming chunks are openai-shaped; the generator is consumed
        # in a worker thread so the event loop is never blocked on the socket.
        import threading

        done = threading.Event()
        error: list[Exception] = []

        def _consume(
            _stream=stream, _acc=acc, _chunks=collected, _err=error, _done=done
        ):
            try:
                for chunk in _stream:
                    for choice in chunk.choices or []:
                        d = getattr(choice, "delta", None)
                        if d is None:
                            continue
                        text = getattr(d, "content", None)
                        if text:
                            _chunks.append(text)
                        tc = getattr(d, "tool_calls", None)
                        if tc:
                            for part in tc:
                                _acc_tool_call(_acc, part)
            except Exception as e:  # noqa: BLE001 - propagate to async side
                _err.append(e)
            finally:
                _done.set()

        t = threading.Thread(target=_consume, daemon=True)
        t.start()
        while not done.wait(0.05):
            await asyncio.sleep(0)
        if error:
            raise error[0]

        text = "".join(collected)
        tool_calls = acc or None

        if not tool_calls:
            answer, confidence, related = parse_final_answer(text)
            yield {"type": "delta", "text": text}
            yield {"type": "sources", "sources": _sources_from_messages(messages)}
            yield {
                "type": "done",
                "answer": answer,
                "related_questions": related,
                "confidence": confidence,
                "model": model_id,
                "cached": False,
            }
            return

        messages.append(
            {
                "role": "assistant",
                "content": text or None,
                "tool_calls": [
                    {
                        "id": tc["id"],
                        "type": "function",
                        "function": {"name": tc["name"], "arguments": tc["arguments"]},
                    }
                    for tc in tool_calls
                ],
            }
        )
        for tc in tool_calls:
            fn = tools.get(tc["name"])
            try:
                args = json.loads(tc["arguments"] or "{}")
            except json.JSONDecodeError:
                args = {}
            yield {
                "type": "step",
                "tool": tc["name"],
                "query": args.get("query") or "",
                "label": f"Searching: {args.get('query') or tc['name']}",
            }
            if fn is None:
                result = {"error": f"unknown tool {tc['name']}"}
            else:
                try:
                    result = fn(**args)
                except Exception as e:
                    log.exception("tool %s failed", tc["name"])
                    result = {"error": str(e)}
            messages.append(
                {
                    "role": "tool",
                    "tool_call_id": tc["id"],
                    "content": json.dumps(result, default=str),
                }
            )

        # Early-confidence exit: if the assistant streamed text alongside tool
        # calls and self-reported confidence >= threshold, finalize without
        # another model round.
        if text:
            _, confidence, related = parse_final_answer(text)
            if confidence >= CONFIDENCE_THRESHOLD:
                yield {"type": "delta", "text": text}
                yield {"type": "sources", "sources": _sources_from_messages(messages)}
                yield {
                    "type": "done",
                    "answer": text,
                    "related_questions": related,
                    "confidence": confidence,
                    "model": model_id,
                    "cached": False,
                }
                return

    # iteration budget exhausted: best-effort final answer from transcript
    yield {
        "type": "done",
        "answer": "",
        "related_questions": [],
        "confidence": 0,
        "model": model_id,
        "cached": False,
        "error": f"exceeded max iterations ({max_iterations}) without a final answer",
    }


def _acc_tool_call(acc: list[dict], part) -> None:
    """Accumulate streamed tool_call deltas (openai-style index merging)."""
    idx = getattr(part, "index", 0) or 0
    while len(acc) <= idx:
        acc.append({"id": "", "name": "", "arguments": ""})
    slot = acc[idx]
    if getattr(part, "id", None):
        slot["id"] = part.id
    fn = getattr(part, "function", None)
    if fn is not None:
        if getattr(fn, "name", None):
            slot["name"] = fn.name
        if getattr(fn, "arguments", None):
            slot["arguments"] += fn.arguments


def sse_format(event: dict) -> str:
    return f"data: {json.dumps(event, separators=(',', ':'))}\n\n"
