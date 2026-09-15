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
# Answers below this confidence are streamed to the user but never cached
# (low-confidence / greeting replies poisoning the cache for 24h).
CACHE_MIN_CONFIDENCE = 4
# Generic assistant greetings that mean the model ignored the query.
_GREETING_SUBSTRINGS = ("how can i help", "what can i help", "what's on your mind")

CONFIDENCE_RE = re.compile(r'"confidence"\s*:\s*(\d+)')

# Some openai-compatible providers (notably OpenRouter free models) emit tool
# calls as inline text in a vendor tag format instead of using the OpenAI
# tool_calls protocol. Detect it so the loop can surface a clear model error
# instead of streaming raw tool-call JSON to the user.
TEXT_TOOL_CALL_RE = re.compile(r"<tool_call>|<arg_key>|</tool_call>")

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
                        "description": (
                            "web = live search, history = user's clicked URLs,"
                            " cache = previously cached searches"
                        ),
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


def _sdk_client(cfg: AIConfig):
    """Build a direct provider SDK client (no aisuite: its Client drops the
    tools kwarg unless max_turns is set, which breaks our own tool loop).

    Returns (client, model_id, kind) where kind is 'openai' or 'anthropic'.
    """
    api_key = cfg.resolve_api_key()
    model_id = f"{cfg.provider}:{cfg.model}"
    if cfg.provider == "anthropic":
        import anthropic  # lazy

        kwargs = {"api_key": api_key} if api_key else {}
        if cfg.base_url:
            kwargs["base_url"] = cfg.base_url
        return anthropic.Anthropic(**kwargs), model_id, "anthropic"

    from openai import OpenAI  # lazy: openai-compatible (incl. OpenRouter etc.)

    kwargs = {"api_key": api_key} if api_key else {}
    if cfg.base_url:
        kwargs["base_url"] = cfg.base_url
    return OpenAI(**kwargs), model_id, "openai"


def _aisuite_client(cfg: AIConfig):
    """Legacy aisuite client path (kept for tests/fallback)."""
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


def _greeting_error(answer: str, confidence: int) -> str | None:
    """Flag a generic greeting that the model emitted instead of an answer."""
    if confidence > 0 or not answer:
        return None
    low = answer.lower()
    if len(low) <= 200 and any(s in low for s in _GREETING_SUBSTRINGS):
        return "model did not answer the query - try a different model"
    return None


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

    client, model_id, sdk_kind = await asyncio.to_thread(_sdk_client, cfg)

    for _iteration in range(max_iterations):
        collected: list[str] = []
        acc: list[dict] = []
        try:
            if sdk_kind == "anthropic":
                stream = await asyncio.to_thread(
                    _anthropic_stream,
                    client,
                    model=cfg.model,
                    system=SYSTEM_PROMPT,
                    messages=messages[1:],  # strip the system entry
                    tools=TOOLS_SPEC,
                )
            else:
                stream = await asyncio.to_thread(
                    client.chat.completions.create,
                    model=cfg.model,
                    messages=messages,
                    tools=TOOLS_SPEC,
                    stream=True,
                )
        except Exception as e:
            yield {
                "type": "done",
                "answer": "",
                "related_questions": [],
                "confidence": 0,
                "model": model_id,
                "cached": False,
                "error": _friendly_provider_error(e, cfg),
            }
            return
        # streaming chunks are openai-shaped (anthropic events are converted
        # by _anthropic_stream); the generator is consumed in a worker thread
        # so the event loop is never blocked on the socket.
        import threading

        done = threading.Event()
        error: list[Exception] = []

        def _consume(_stream=stream, _acc=acc, _chunks=collected, _err=error, _done=done):
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
            except Exception as e:
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

        # Text-embedded tool calls: the model answered in a vendor tag format
        # (e.g. <tool_call>...<arg_key>...) instead of the tool_calls protocol.
        # This means the configured model does not support function calling on
        # this provider: surface it as a clean, actionable error.
        if not tool_calls and TEXT_TOOL_CALL_RE.search(text):
            log.warning(
                "stream_answer: model %s emitted text tool calls (no tool_calls "
                "support); aborting with a model error",
                model_id,
            )
            yield {
                "type": "done",
                "answer": "",
                "related_questions": [],
                "confidence": 0,
                "model": model_id,
                "cached": False,
                "error": (
                    f"model '{cfg.model}' does not support tool calling on this "
                    "provider - pick a model with function calling enabled "
                    "(free/openrouter models often do not)"
                ),
            }
            return

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
                "error": _greeting_error(answer, confidence),
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


def _friendly_provider_error(e: Exception, cfg: AIConfig) -> str:
    """Translate provider exceptions into short, actionable user messages."""
    s = str(e)
    low = s.lower()
    if "401" in s or "unauthorized" in low or "invalid api key" in low or "auth" in low.split():
        return "401 unauthorized - check the api key in settings"
    if "404" in s and ("model" in low or "endpoint" in low):
        return f"model '{cfg.model}' not found on this provider - check the model id"
    if "404" in s:
        return "provider endpoint not found - check the base url in settings"
    if "429" in s or "rate" in low:
        return "rate limited (429) by the provider - try again shortly"
    if "402" in s or "credit" in low or "quota" in low:
        return "provider rejected the request (credits/quota exhausted)"
    if "timeout" in low or "timed out" in low:
        return "provider request timed out"
    return f"provider error: {s[:200]}"


async def test_provider_connection(cfg: AIConfig, timeout_s: float = 10.0) -> dict:
    """Verify provider + api key + base_url + model actually work.

    Returns {ok, detail}. Strategy per provider:
      - openai-compatible: try a 1-token chat completion; on failure fall back
        to models.list (some models gate completions but list fine, and a
        completed call is the stronger signal).
      - anthropic: models.list then a 1-token message.
    """
    import asyncio

    api_key = cfg.resolve_api_key()
    if not api_key and cfg.provider != "ollama":
        return {"ok": False, "detail": "no api key configured - set one in settings"}

    async def _check() -> dict:
        if cfg.provider == "anthropic":
            import anthropic  # lazy

            client = anthropic.Anthropic(api_key=api_key, timeout=timeout_s)
            client.models.list(limit=1)
            msg = client.messages.create(
                model=cfg.model,
                max_tokens=1,
                messages=[{"role": "user", "content": "hi"}],
            )
            return {
                "ok": True,
                "detail": f"ok: completion succeeded (model replied {len(msg.content)} block(s))",
            }

        from openai import OpenAI  # lazy

        client = OpenAI(api_key=api_key, base_url=cfg.base_url, timeout=timeout_s)
        try:
            client.chat.completions.create(
                model=cfg.model,
                max_tokens=1,
                messages=[{"role": "user", "content": "hi"}],
            )
            return {"ok": True, "detail": "ok: completion succeeded"}
        except Exception as completion_err:
            # fall back to listing: distinguishes auth/base_url issues from
            # a bad model id (a 401 fails both; a bad model only the call)
            try:
                models = client.models.list()
                if not models.data:
                    raise _ModelNotFound("provider model list is empty")  # noqa: TRY301
                raise _ModelNotFound(str(completion_err))  # noqa: TRY301
            except _ModelNotFound:
                raise
            except Exception as list_err:
                # listing succeeded but completion failed -> model problem
                if not _looks_like_auth(list_err):
                    return {
                        "ok": False,
                        "detail": _friendly_provider_error(completion_err, cfg),
                    }
                raise

    def _check_sync() -> dict:
        """Run the (blocking) SDK checks on a worker thread with its own loop."""
        import asyncio

        return asyncio.run(_check())

    try:
        return await asyncio.wait_for(asyncio.to_thread(_check_sync), timeout_s + 5)
    except Exception as e:
        return {"ok": False, "detail": _friendly_provider_error(e, cfg)}


class _ModelNotFound(Exception):
    pass


def _looks_like_auth(e: Exception) -> bool:
    s = str(e)
    return "401" in s or "unauthorized" in s.lower()


def _anthropic_stream(client, model, system, messages, tools):
    """Run an anthropic streaming call and yield openai-shaped chunk objects.

    Each yielded item is SimpleNamespace(choices=[SimpleNamespace(delta=...)])`
    so the shared consumer loop handles both providers.
    """
    from types import SimpleNamespace

    def _chunk(delta):
        return SimpleNamespace(choices=[SimpleNamespace(delta=delta)])

    openai_tools = [
        {
            "name": t["function"]["name"],
            "description": t["function"]["description"],
            "input_schema": t["function"]["parameters"],
        }
        for t in tools
    ]
    with client.messages.stream(
        model=model,
        system=system,
        messages=messages,
        max_tokens=2048,
        tools=openai_tools,
    ) as stream:
        current_block = {"type": None, "name": "", "arguments": "", "index": 0}
        for event in stream:
            et = getattr(event, "type", None)
            if et == "content_block_start":
                block = getattr(event, "content_block", None)
                btype = getattr(block, "type", None)
                if btype == "tool_use":
                    current_block = {
                        "type": "tool_use",
                        "name": getattr(block, "name", ""),
                        "arguments": "",
                        "index": getattr(event, "index", 0),
                        "id": getattr(block, "id", ""),
                    }
                    yield _chunk(
                        SimpleNamespace(
                            content=None,
                            tool_calls=[
                                SimpleNamespace(
                                    id=block.id,
                                    index=current_block["index"],
                                    function=SimpleNamespace(name=block.name, arguments=""),
                                )
                            ],
                        )
                    )
                elif btype == "text":
                    current_block = {"type": "text", "name": "", "arguments": "", "index": 0}
            elif et == "content_block_delta":
                delta = getattr(event, "delta", None)
                dtype = getattr(delta, "type", None)
                if dtype == "text_delta":
                    yield _chunk(SimpleNamespace(content=delta.text, tool_calls=None))
                elif dtype == "input_json_delta":
                    current_block["arguments"] += delta.partial_json
                    yield _chunk(
                        SimpleNamespace(
                            content=None,
                            tool_calls=[
                                SimpleNamespace(
                                    id=None,
                                    index=current_block["index"],
                                    function=SimpleNamespace(
                                        name=None, arguments=delta.partial_json
                                    ),
                                )
                            ],
                        )
                    )


def sse_format(event: dict) -> str:
    return f"data: {json.dumps(event, separators=(',', ':'))}\n\n"
