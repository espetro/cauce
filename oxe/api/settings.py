"""``GET|PUT /settings``: typed HTTP boundary over ``oxe.config``.

The payload (``SettingsPayload``) mirrors ``AIConfig``'s field set exactly
(``provider``, ``model``, ``api_key``, ``api_key_env``, ``base_url``,
``enabled``) with one addition: ``api_key_set``. GET must never echo a raw
secret, so ``api_key`` is always ``null`` in responses and ``api_key_set``
carries the presence bit instead. PUT accepts ``api_key`` and persists it the
one way ``oxe.config`` already supports (the ``[ai] api_key`` key of
``config.toml``, or an env-var name via ``api_key_env``) -- no new secret
store is invented here.

Legacy bug guard: legacy's ``PUT /settings`` allowed extra fields silently and
wiped ``base_url`` for 10h. Both directions are ``extra="forbid"`` with
``provider``/``model`` required, so a PUT that omits fields or carries unknown
ones is a 422, never a silent partial overwrite.

Two wire models, one shape: ``SettingsPayload`` (GET/response, unconstrained
strings) and ``SettingsWritePayload`` (PUT body, provider constrained to
``VALID_PROVIDERS`` and a non-empty model). The split exists because the
unconfigured ``GET /settings`` response is a blank form (empty provider/model),
which must stay schema-valid -- but the same values must never be *saved* via
PUT, so the write model rejects them at pydantic validation time (422), before
any handler code runs.
"""

import asyncio
from typing import Literal

from fastapi import APIRouter
from pydantic import BaseModel, ConfigDict, Field

from oxe.config import AIConfig, load_config, save_config

router = APIRouter()

_ProviderLiteral = Literal["anthropic", "groq", "huggingface", "mistral", "ollama", "openai"]


class SettingsPayload(BaseModel):
    """Wire mirror of ``AIConfig``; never carries a raw secret outbound."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    provider: str
    model: str
    api_key: str | None = None
    api_key_set: bool = False
    api_key_env: str | None = None
    base_url: str | None = None
    enabled: bool = True


class SettingsWritePayload(SettingsPayload):
    """PUT body: same fields, but blank forms are invalid here.

    ``provider`` is narrowed to the same set as ``oxe.config``'s
    ``VALID_PROVIDERS`` (which the ``_ProviderLiteral`` literal mirrors) and
    ``model`` must be non-empty: a value the next ``load_config`` would reject
    (500 on every later GET) must never be persisted. Response payloads keep
    the unconstrained ``SettingsPayload`` so the unconfigured blank form stays
    schema-valid. Narrowing (not redeclaring) the inherited fields keeps the
    single source of truth for the field list.
    """

    provider: _ProviderLiteral  # pyright: ignore[reportIncompatibleVariableOverride]
    model: str = Field(min_length=1)


def _to_payload(cfg: AIConfig) -> SettingsPayload:
    """Narrow a validated ``AIConfig`` into the response payload.

    ``model_construct``: ``load_config`` has already validated provider
    against ``VALID_PROVIDERS``, so the runtime value always satisfies the
    write model's provider Literal; strict construction would demand that
    narrowing be re-proven here for no additional safety.
    """
    return SettingsPayload.model_construct(
        provider=cfg.provider,
        model=cfg.model,
        api_key=None,
        api_key_set=cfg.api_key is not None,
        api_key_env=cfg.api_key_env,
        base_url=cfg.base_url,
        enabled=cfg.enabled,
    )


def _get_sync() -> SettingsPayload:
    """Blocking tail of ``GET /settings``, executed via ``asyncio.to_thread``.

    No config file yet is not an error: the blank-form payload (empty
    provider/model) is returned so the settings dialog starts from a blank
    form and schema-compliant GETs always get a 200. Only a malformed config
    file raises ``ConfigError``.
    """
    cfg = load_config()
    if cfg is None:
        return SettingsPayload(provider="", model="")
    return _to_payload(cfg)


def _put_sync(payload: SettingsWritePayload) -> SettingsPayload:
    """Blocking tail of ``PUT /settings``, executed via ``asyncio.to_thread``."""
    cfg = AIConfig(
        provider=payload.provider,
        model=payload.model,
        api_key=payload.api_key,
        api_key_env=payload.api_key_env,
        base_url=payload.base_url,
        enabled=payload.enabled,
    )
    save_config(cfg)
    return _to_payload(cfg)


@router.get("/settings", response_model=SettingsPayload, summary="Current AI config.")
async def get_settings() -> SettingsPayload:
    """The persisted ``[ai]`` config; ``api_key`` is never echoed."""
    return await asyncio.to_thread(_get_sync)


@router.put("/settings", response_model=SettingsPayload, summary="Persist the AI config.")
async def put_settings(payload: SettingsWritePayload) -> SettingsPayload:
    """Full-replace semantics: the body must carry every field (422 otherwise)."""
    return await asyncio.to_thread(_put_sync, payload)
