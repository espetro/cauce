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
from fastapi.exceptions import RequestValidationError
from pydantic import BaseModel, ConfigDict

from oxe.config import VALID_PROVIDERS, AIConfig, load_config, save_config

router = APIRouter()


class SettingsPayload(BaseModel):
    """Wire mirror of ``AIConfig``; never carries a raw secret outbound.

    ``provider``/``model`` stay plain ``str`` (no ``Literal``/``min_length``)
    so the unconfigured blank form (empty strings from ``GET /settings`` when
    no config exists yet) is a schema-valid response; PUT re-validates the
    saved state in the handler instead, so an invalid provider or empty model
    still never reaches ``config.toml`` (see ``_put_sync``).
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    provider: str
    model: str
    api_key: str | None = None
    api_key_set: bool = False
    api_key_env: str | None = None
    base_url: str | None = None
    enabled: bool = True


def _to_payload(cfg: AIConfig) -> SettingsPayload:
    # model_construct: load_config has already validated provider against
    # VALID_PROVIDERS, so the runtime value always satisfies the payload's
    # provider Literal; strict construction would demand that narrowing be
    # re-proven here for no additional safety.
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

    No config file yet is not an error: an empty provider/model payload is
    returned so the settings dialog starts from a blank form (and schemathesis
    never sees a 5xx from a spec-valid GET). Only a malformed config file
    raises ``ConfigError``.
    """
    cfg = load_config()
    if cfg is None:
        # Unconfigured blank form: empty provider/model, always schema-valid
        # against the plain-str payload above.
        return SettingsPayload(provider="", model="")
    return _to_payload(cfg)


def _put_sync(payload: SettingsPayload) -> SettingsPayload:
    """Blocking tail of ``PUT /settings``, executed via ``asyncio.to_thread``.

    Re-validates provider/model here (not in the pydantic model, whose schema
    must also admit the GET blank form): a value the next ``load_config``
    would reject must never be persisted.
    """
    if payload.provider not in VALID_PROVIDERS:
        msg = (
            f"provider {payload.provider!r} not supported; "
            f"expected one of {', '.join(sorted(VALID_PROVIDERS))}"
        )
        raise RequestValidationError(
            [
                {
                    "type": "value_error",
                    "loc": ("body", "provider"),
                    "input": payload.provider,
                    "ctx": {"error": ValueError(msg)},
                }
            ]
        )
    if not payload.model.strip():
        msg = "model must be a non-empty string"
        raise RequestValidationError(
            [
                {
                    "type": "value_error",
                    "loc": ("body", "model"),
                    "input": payload.model,
                    "ctx": {"error": ValueError(msg)},
                }
            ]
        )
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
