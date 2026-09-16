"""Pydantic contract models for the HTTP API (source of truth for openapi.json).

Notes:
- Response models tolerate extra keys (``extra="allow"``) so provider/backend
  passthrough fields never get stripped.
- ``SearchResponse`` uses serialization aliases for the ``_``-prefixed cache
  transparency fields (pydantic v2 forbids leading-underscore field names).
"""

from typing import Any, Literal, Optional

from pydantic import BaseModel, Field

# ---------------------------------------------------------------- search


class ContentsModel(BaseModel):
    text: Any = None
    highlights: Any = None
    summary: Any = None

    model_config = {"extra": "allow"}


class ExaRequest(BaseModel):
    query: str
    type: Optional[str] = "auto"
    numResults: Optional[int] = 10
    page: int = Field(default=1, ge=1)
    category: Optional[str] = None
    userLocation: Optional[str] = None
    includeDomains: Optional[list[str]] = None
    excludeDomains: Optional[list[str]] = None
    startPublishedDate: Optional[str] = None
    endPublishedDate: Optional[str] = None
    contents: Optional[ContentsModel] = None
    additionalQueries: Optional[list[str]] = None
    systemPrompt: Optional[str] = None
    outputSchema: Optional[dict] = None
    stream: Optional[bool] = None

    model_config = {"extra": "allow"}


class SearchResultItem(BaseModel):
    """Exa-shaped result row; DDG translation fills most fields."""

    title: Optional[str] = None
    url: Optional[str] = None
    id: Optional[str] = None
    text: Optional[str] = None
    highlights: Optional[list[Any]] = None
    highlightScores: Optional[list[Any]] = None
    publishedDate: Optional[str] = None
    author: Optional[str] = None
    image: Optional[str] = None
    favicon: Optional[str] = None
    extras: Optional[dict] = None

    model_config = {"extra": "allow"}


class SearchResponse(BaseModel):
    """Search payload shared by POST /search and the GET /search JSON branch.

    ``_``-prefixed fields are cache-transparency metadata; they are declared
    with serialization aliases because pydantic v2 forbids leading underscores
    in field names. Serialization must round-trip them byte-identically.
    """

    requestId: Optional[str] = None
    searchType: Optional[str] = None
    results: list[SearchResultItem] = []
    resolvedSearchType: Optional[str] = None
    costDollars: Optional[dict] = None

    # cache transparency (aliases; never accepted as python names on input)
    source: Optional[str] = Field(
        default=None, serialization_alias="_source", validation_alias="_source"
    )
    q_hash: Optional[str] = Field(
        default=None, serialization_alias="_q_hash", validation_alias="_q_hash"
    )
    q: Optional[str] = Field(
        default=None, serialization_alias="_q", validation_alias="_q"
    )
    backend: Optional[str] = Field(
        default=None, serialization_alias="_backend", validation_alias="_backend"
    )
    duration_ms: Optional[int] = Field(
        default=None, serialization_alias="_duration_ms", validation_alias="_duration_ms"
    )
    cached_at: Optional[int] = Field(
        default=None, serialization_alias="_cached_at", validation_alias="_cached_at"
    )
    error: Optional[str] = Field(
        default=None, serialization_alias="_error", validation_alias="_error"
    )
    error_kind: Optional[str] = Field(
        default=None, serialization_alias="_error_kind", validation_alias="_error_kind"
    )
    page: Optional[int] = Field(
        default=None, serialization_alias="_page", validation_alias="_page"
    )

    model_config = {"extra": "allow", "populate_by_name": True}


# ---------------------------------------------------------------- errors


class ErrorDetail(BaseModel):
    code: str
    message: str


class ErrorEnvelope(BaseModel):
    error: ErrorDetail


# ---------------------------------------------------------------- health


class HealthResponse(BaseModel):
    status: str
    service: str
    cache_size: int
    version: str
    pid: int


# ---------------------------------------------------------------- settings


class AISettingsPayload(BaseModel):
    """Body of the ``ai`` object for PUT /settings and POST /settings/test.

    ``{env.*}`` template strings are preserved verbatim (plain str type);
    api_key/base_url omitted means "keep the stored value" (handled in the
    route, not here).
    """

    provider: str = ""
    model: str = ""
    api_key: Optional[str] = None
    api_key_env: Optional[str] = None
    base_url: Optional[str] = None
    enabled: bool = True


class SettingsPutRequest(BaseModel):
    ai: AISettingsPayload


class SettingsTestRequest(BaseModel):
    ai: AISettingsPayload


class SettingsTestResponse(BaseModel):
    ok: bool
    detail: str


class SettingsPutResponse(BaseModel):
    ok: bool
    config_path: str


class AISettingsView(BaseModel):
    provider: str
    model: str
    api_key_set: bool
    api_key_env: Optional[str] = None
    base_url: Optional[str] = None
    enabled: bool


class SettingsResponse(BaseModel):
    configured: bool
    config_path: str
    ai: Optional[AISettingsView] = None


# ---------------------------------------------------------------- answer / models


class AnswerRequest(BaseModel):
    query: str = Field(min_length=1)


class ModelItem(BaseModel):
    id: str

    model_config = {"extra": "allow"}


class ModelsResponse(BaseModel):
    object: str
    data: list[ModelItem]
    ai_available: bool
    error: Optional[str] = None


# ---------------------------------------------------------------- cache / history


class CacheStatsResponse(BaseModel):
    rows: int
    unexpired_rows: int
    db_size_bytes: int
    total_hits: int
    oldest_unexpired: Optional[int] = None
    newest: Optional[int] = None


class CacheInvalidateResponse(BaseModel):
    deleted: int


class ClickItem(BaseModel):
    kind: Literal["click"]
    clicked_at: int
    query_hash: str
    query: str
    result_id: str
    url: str
    title: str
    source: str

    model_config = {"extra": "allow"}


class CacheItem(BaseModel):
    kind: Literal["cache"]
    created_at: int
    query_hash: str
    query: str
    expires_at: int
    hits: int
    size_bytes: int

    model_config = {"extra": "allow"}


class ApiHistoryResponse(BaseModel):
    items: list[ClickItem | CacheItem]
    clicks: int
    cache_rows: int
    limit: int
    since: str


class ApiStatsResponse(BaseModel):
    """Dashboard aggregates; stats.py owns the exact shape, extras allowed."""

    days: int

    model_config = {"extra": "allow"}


class ClickRequest(BaseModel):
    query_hash: str
    result_id: str
    url: str
    title: str = ""
    source: Literal["web-ui", "mcp"] = "web-ui"


class ClickResponse(BaseModel):
    ok: bool
    click_id: int


class HistoryDeleteRequest(BaseModel):
    scope: Literal["24h", "7d", "30d", "all"] = "all"


class HistoryDeleteResponse(BaseModel):
    ok: bool
    deleted: int
