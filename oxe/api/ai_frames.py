"""Typed SSE frame union for ``POST /answer`` (AI answers), a later wave.

Per the plan's "the SSE hole" section: ``text/event-stream`` routes have no
route-level ``response_model`` FastAPI can introspect (that's what a normal
JSON response uses to populate ``openapi.json``), so nothing generates a TS
type for the frames unless the frame *types* are declared and registered as
OpenAPI components explicitly, ahead of the route that will eventually emit
them. This module does exactly that and nothing else: no ``POST /answer``
route exists yet (that's wave 4's ``oxe/ai.py`` rewrite), so
``register_answer_frame_schemas`` is the only thing wired into
``oxe.app.create_app`` from here.

Frame kinds mirror legacy ``oxe/ai.py``'s docstring almost exactly
(``step``, ``delta``, ``sources``, ``done``), each event dict there tagged
with a ``"type"`` key -- that shape is kept as the discriminator field on a
proper pydantic union instead of an untyped dict literal. One frame kind is
added beyond legacy: ``error``, for a transport-level failure (provider
unreachable, auth failure) that happens *before* any ``done`` frame can be
constructed. Legacy folded this into an optional ``error`` field on ``done``
instead (see the two ``yield {"type": "done", ..., "error": ...}`` sites in
its stream loop); that convention is kept for a *mid-stream* failure (the
model produced a low-confidence or malformed final answer), but a standalone
``error`` frame kind is added for failures that abort the stream before a
``done`` frame would otherwise have every other field populated -- typing
that case as "a done frame with every other field faked to a placeholder"
would be worse than a dedicated frame kind for it.
"""

from typing import Annotated, Literal, cast

from fastapi import FastAPI
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from oxe.jsontypes import JSONDict


class StepFrame(BaseModel):
    """A tool call the model is making (e.g. a web search) is starting."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    type: Literal["step"] = "step"
    tool: str
    query: str
    label: str


class DeltaFrame(BaseModel):
    """One chunk of streamed answer text."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    type: Literal["delta"] = "delta"
    text: str


class AnswerSource(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    title: str
    url: str
    favicon: str | None = None


class SourcesFrame(BaseModel):
    """The set of sources the answer drew on, once known."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    type: Literal["sources"] = "sources"
    sources: list[AnswerSource] = Field(default_factory=list)


class DoneFrame(BaseModel):
    """Terminal frame: the final answer, or a mid-stream model/quality failure."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    type: Literal["done"] = "done"
    answer: str
    related_questions: list[str] = Field(default_factory=list)
    confidence: int
    model: str
    cached: bool
    error: str | None = None


class ErrorFrame(BaseModel):
    """Terminal frame for a failure before any ``done`` frame could be built."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    type: Literal["error"] = "error"
    message: str


AnswerFrame = Annotated[
    StepFrame | DeltaFrame | SourcesFrame | DoneFrame | ErrorFrame,
    Field(discriminator="type"),
]

_FRAME_COMPONENT_NAME = "AnswerFrame"
_SSE_ENVELOPE_NAME = "AnswerFrameEvent"


def register_answer_frame_schemas(app: FastAPI) -> None:
    """Injects the ``AnswerFrame`` union into ``app``'s OpenAPI components.

    Called once from ``oxe.app.create_app``, after all routers are mounted
    (so the schema this computes already reflects the final route set).
    ``app.openapi()`` returns FastAPI's own ``dict[str, Any]`` -- narrowed
    into ``JSONDict`` immediately via ``cast``, the same boundary pattern
    ``oxe.config._as_object_dict`` uses for ``tomllib``'s untyped return.
    """
    schema = cast(JSONDict, app.openapi())
    components_raw = schema.setdefault("components", {})
    components = cast(JSONDict, components_raw)
    schemas_raw = components.setdefault("schemas", {})
    schemas = cast(JSONDict, schemas_raw)

    frame_schema = cast(
        JSONDict,
        TypeAdapter(AnswerFrame).json_schema(ref_template="#/components/schemas/{model}"),
    )
    defs = cast(JSONDict, frame_schema.pop("$defs", {}))
    schemas.update(defs)
    schemas[_FRAME_COMPONENT_NAME] = frame_schema

    # SSE envelope schema: each ``data:`` event is described by wrapping the
    # frame union in ``contentMediaType``/``contentSchema`` (JSON Schema's
    # standard way to type a string that carries an embedded JSON payload).
    # schemathesis's SSE conformance check validates event data through this
    # envelope, and openapi-typescript generates the payload type from
    # ``contentSchema``. Refs stay document-relative
    # (``#/components/schemas/...``), which is correct once this component
    # lands inside the OpenAPI document.
    schemas[_SSE_ENVELOPE_NAME] = {
        "type": "object",
        "properties": {
            "event": {"type": "string", "const": "message"},
            "data": {
                "contentMediaType": "application/json",
                "contentSchema": {"$ref": "#/components/schemas/" + _FRAME_COMPONENT_NAME},
            },
        },
        "required": ["data"],
    }

    app.openapi_schema = schema
