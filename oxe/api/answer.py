"""``POST /answer``: AI answer SSE route over the typed frame union.

Emits ``text/event-stream`` frames from ``oxe/api/ai_frames.py`` (the
frames are registered OpenAPI components even though the stream itself has
no JSON schema -- the plan's "the SSE hole" section). The request body and
the non-streaming ``ai-off`` branch are typed normally.

QA fixtures (day-0 gate 6): ``force=ai-off|error|empty`` in the body
produce deterministic frame sequences without any provider call, mirroring
the UI's ``force`` contract in ``ui/src/lib/fixtures.ts``:

- ``ai-off``: a single ``ErrorFrame`` ("AI unavailable") -- the "AI mode is
  not configured" state checkpoint 12 renders.
- ``error``: a single ``ErrorFrame`` with a generic provider-failure message.
- ``empty``: ``SourcesFrame`` with zero sources, then ``DoneFrame`` with an
  empty answer and confidence 0.

Real answers are cached in the answers table keyed on (query, model), only
when the model self-reported ``confidence >= CACHE_MIN_CONFIDENCE`` and no
error was set; a hit replays ``DoneFrame`` + ``SourcesFrame`` (in that
order) with ``cached=True`` and never calls the provider. All sqlite
access runs behind ``asyncio.to_thread`` (ASYNC rule).
"""

import asyncio
from collections.abc import AsyncGenerator
from typing import Annotated, Literal

from fastapi import APIRouter, Depends, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from oxe.ai import answer_cache_key, build_toolset, stream_answer
from oxe.api.ai_frames import AnswerFrame, DeltaFrame, DoneFrame, ErrorFrame, SourcesFrame
from oxe.api.searx import SearchServiceDep
from oxe.api.stats import CacheDep
from oxe.cache import TTLCache
from oxe.config import CACHE_MIN_CONFIDENCE, AIConfig, load_config
from oxe.jsontypes import JSONDict

router = APIRouter()

ANSWER_TTL_S = 24 * 3600

AI_OFF_MESSAGE = "AI unavailable - no AI provider is configured in settings"
ERROR_FIXTURE_MESSAGE = "simulated provider error (force=error QA fixture)"
ERROR_FIXTURE_PARTIAL = "The asyncio event loop schedules coroutines and"

_FRAME_ADAPTER: TypeAdapter[AnswerFrame] = TypeAdapter(AnswerFrame)


class AnswerRequest(BaseModel):
    """Body of ``POST /answer``."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    query: str = Field(min_length=1)
    mode: Literal["answer"] = "answer"
    # QA fixture: mirrors ui/src/lib/fixtures.ts ForcedState. None = normal.
    force: Literal["ai-off", "error", "empty"] | None = None


class CachedAnswer(BaseModel):
    """Payload stored in the answers table (one cached final answer)."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    done: DoneFrame
    sources: SourcesFrame


def get_ai_config(request: Request) -> AIConfig | None:
    """Per-request ``load_config()``; ``None`` means AI mode is off.

    Read per request (not once at app-factory time) so settings saved
    through a settings route apply without a process restart. A missing
    config.toml, ``[ai]`` section or ``enabled = false`` all mean off --
    the route answers with the ``ai-off`` ``ErrorFrame``, never an HTTP
    error.
    """
    del request
    return load_config()


AIConfigDep = Annotated[AIConfig | None, Depends(get_ai_config)]


async def _fixture_frames(req: AnswerRequest) -> AsyncGenerator[AnswerFrame, None]:
    if req.force == "ai-off":
        yield ErrorFrame(message=AI_OFF_MESSAGE)
    elif req.force == "error":
        yield DeltaFrame(text=ERROR_FIXTURE_PARTIAL)
        yield ErrorFrame(message=ERROR_FIXTURE_MESSAGE)
    else:  # force == "empty"
        yield SourcesFrame(sources=[])
        yield DoneFrame(answer="", confidence=0, model="fixture", cached=False)


async def _answer_frames(
    req: AnswerRequest,
    cfg: AIConfig | None,
    service: SearchServiceDep,
    cache: TTLCache,
) -> AsyncGenerator[AnswerFrame, None]:
    """The full frame stream: fixtures, cache replay, or the live loop."""
    if req.force is not None:
        async for frame in _fixture_frames(req):
            yield frame
        return

    if cfg is None:
        yield ErrorFrame(message=AI_OFF_MESSAGE)
        return

    model_id = f"{cfg.provider}:{cfg.model}"
    key = answer_cache_key(req.query, model_id)
    cached_raw = await asyncio.to_thread(cache.get_answer, key)
    if cached_raw is not None:
        cached = CachedAnswer.model_validate(cached_raw)
        yield cached.done.model_copy(update={"cached": True})
        yield cached.sources
        return

    tools = build_toolset(service, cache)
    terminal: DoneFrame | None = None
    sources = SourcesFrame()
    async for frame in stream_answer(req.query, cfg, tools):
        if isinstance(frame, DoneFrame):
            terminal = frame
            continue
        if isinstance(frame, SourcesFrame):
            sources = frame
        yield frame

    if terminal is None:  # pragma: no cover - stream_answer always ends Done
        return
    if terminal.error is None and terminal.confidence >= CACHE_MIN_CONFIDENCE:
        payload = CachedAnswer(done=terminal, sources=sources)
        await asyncio.to_thread(
            cache.put_answer,
            key,
            req.query,
            _as_json_dict(payload.model_dump(mode="json")),
            ANSWER_TTL_S,
            model_id,
        )
    yield terminal


def _as_json_dict(value: JSONDict) -> JSONDict:
    """``model_dump(mode="json")`` output is JSON by construction; narrowed
    once here for the ``TTLCache.put_answer(JSONDict)`` boundary."""
    return value


def _sse(frame: AnswerFrame) -> str:
    return f"data: {frame.model_dump_json()}\n\n"


async def _sse_stream(
    frames: AsyncGenerator[AnswerFrame, None],
) -> AsyncGenerator[str, None]:
    async for frame in frames:
        yield _sse(frame)


@router.post(
    "/answer",
    summary="AI answer (SSE stream of typed frames).",
    # The stream itself has no JSON schema FastAPI can infer (the plan's
    # "SSE hole"); the frames are registered as components by
    # ``register_answer_frame_schemas``, so the 200 points at the
    # AnswerFrame $ref under the SSE media type.
    responses={
        200: {
            "description": "SSE stream of AnswerFrame events.",
            "content": {
                "text/event-stream": {"schema": {"$ref": "#/components/schemas/AnswerFrameEvent"}}
            },
        }
    },
)
async def answer(
    req: AnswerRequest,
    cfg: AIConfigDep,
    service: SearchServiceDep,
    cache: CacheDep,
) -> StreamingResponse:
    return StreamingResponse(
        _sse_stream(_answer_frames(req, cfg, service, cache)),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )
