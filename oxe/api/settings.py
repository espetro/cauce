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
wiped ``base_url`` for 10h. ``SettingsPayload`` is ``extra="forbid"`` with
``provider``/``model`` required, so a PUT that omits fields or carries unknown
ones is a 422, never a silent partial overwrite.
"""

import asyncio

from fastapi import APIRouter
from pydantic import BaseModel, ConfigDict

from oxe.config import AIConfig, ConfigError, load_config, save_config

router = APIRouter()


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


def _to_payload(cfg: AIConfig) -> SettingsPayload:
    return SettingsPayload(
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

    No config file yet is not an error: an empty provider/model payload is
    returned so the settings dialog starts from a blank form (and schemathesis
    never sees a 5xx from a spec-valid GET). Only a malformed config file
    raises ``ConfigError``.
    """
    cfg = load_config()
    if cfg is None:
        return SettingsPayload(provider="", model="")
    return _to_payload(cfg)


def _put_sync(payload: SettingsPayload) -> SettingsPayload:
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
async def put_settings(payload: SettingsPayload) -> SettingsPayload:
    """Full-replace semantics: the body must carry every field (422 otherwise)."""
    return await asyncio.to_thread(_put_sync, payload)
