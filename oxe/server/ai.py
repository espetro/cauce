import logging
import time

from fastapi import APIRouter, HTTPException
from fastapi.responses import Response, StreamingResponse

from ..devlog import DEV as _DEV
from ..devlog import event as _dev_event
from .schemas import (
    AnswerRequest,
    ModelsResponse,
    SettingsPutRequest,
    SettingsPutResponse,
    SettingsResponse,
    SettingsTestRequest,
    SettingsTestResponse,
)
from .state import AppState

log = logging.getLogger(__name__)


def _run_async(coro_fn):
    """Run an async SDK call from a sync route handler (worker-thread safe).

    ``coro_fn`` is a zero-arg callable returning an awaitable, so the
    coroutine is created inside the loop that runs it.
    """
    import asyncio

    async def _await_it():
        return await coro_fn()

    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(_await_it())
    # A loop is already running in this thread (defensive; sync def routes
    # run in worker threads). Use a dedicated loop on a clean thread.
    import concurrent.futures

    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as ex:
        return ex.submit(asyncio.run, _await_it()).result()


def build_router(state: AppState) -> APIRouter:
    router = APIRouter()
    c = state.cache

    @router.post("/answer")
    def ai_answer(req: AnswerRequest) -> Response:
        """SSE-stream an AI answer for a query. Requires a configured model."""
        from .. import ai as ai_mod
        from ..config import load_config

        query = req.query.strip()
        if not query:
            raise HTTPException(status_code=422, detail="query required")
        try:
            cfg = load_config()
        except ValueError as e:
            raise HTTPException(status_code=500, detail=f"bad config: {e}")
        if cfg is None:
            raise HTTPException(status_code=409, detail="ai not configured")

        key = ai_mod.answer_cache_key(query, f"{cfg.provider}:{cfg.model}")
        cached = c.get_answer(key)
        if cached is not None:
            payload = dict(cached)
            payload["cached"] = True

            async def _cached_stream():
                if payload.get("sources"):
                    yield ai_mod.sse_format({"type": "sources", "sources": payload["sources"]})
                yield ai_mod.sse_format({"type": "done", **payload})

            if _DEV:
                _dev_event("answer", query=query, cached=True, steps=0)
            return StreamingResponse(_cached_stream(), media_type="text/event-stream")

        async def _stream():
            _t0 = time.monotonic()
            _sources: list = []
            _steps = 0
            _err = None
            final = None
            async for event in ai_mod.stream_answer(
                query, cfg, cache=c, backend=state.backend, on_result=state.observer
            ):
                if event.get("type") == "step":
                    _steps += 1
                if event.get("type") == "sources":
                    _sources = event.get("sources") or []
                if event.get("type") == "done":
                    final = event
                    _err = event.get("error")
                yield ai_mod.sse_format(event)
            if _DEV:
                _dev_event(
                    "answer",
                    query=query,
                    cached=False,
                    steps=_steps,
                    duration_ms=int((time.monotonic() - _t0) * 1000),
                    error=_err,
                )
            if (
                final
                and final.get("answer")
                and not final.get("error")
                and final.get("confidence", 0) >= ai_mod.CACHE_MIN_CONFIDENCE
            ):
                try:
                    ttl = min(ai_mod.ANSWER_TTL_DEFAULT, ai_mod.ANSWER_TTL_MAX)
                    final["sources"] = _sources
                    c.put_answer(key, query, final, ttl, model=final.get("model") or "")
                except Exception:
                    log.exception("failed to cache AI answer")

        return StreamingResponse(_stream(), media_type="text/event-stream")

    @router.get("/v1/models", response_model=ModelsResponse)
    def list_models() -> dict:
        """Provider model listing via direct SDK calls; [] when unconfigured."""
        from ..config import load_config

        def _err(e: Exception) -> dict:
            hint = str(e)[:120]
            return {"object": "list", "data": [], "ai_available": False, "error": hint}

        try:
            cfg = load_config()
        except ValueError as e:
            log.warning("/v1/models: bad config: %s", e)
            return _err(e)
        if cfg is None:
            _dev_event("models", results=0, error="not configured")
            return {"object": "list", "data": [], "ai_available": False}

        api_key = cfg.resolve_api_key()
        models: list[dict] = []
        try:
            if cfg.provider == "openai":
                from openai import AsyncOpenAI  # lazy

                async def _fetch():
                    async with AsyncOpenAI(api_key=api_key, base_url=cfg.base_url) as client:
                        resp = await client.models.list()
                    return [
                        {"id": m.id, "object": "model", "owned_by": getattr(m, "owned_by", None)}
                        for m in resp.data
                    ]

                models = _run_async(_fetch)
            elif cfg.provider == "anthropic":
                from anthropic import AsyncAnthropic  # lazy

                async def _fetch():
                    async with AsyncAnthropic(api_key=api_key, base_url=cfg.base_url) as client:
                        resp = await client.models.list()
                    return [{"id": m.id, "object": "model"} for m in resp.data]

                models = _run_async(_fetch)
        except ImportError:
            log.warning("/v1/models: provider SDK not installed (pip install oxe[ai])")
            _dev_event("models", results=0, error="sdk not installed")
        except Exception as e:
            log.warning("/v1/models: provider listing failed: %s", e)
            _dev_event("models", results=0, error=str(e)[:200])
            return _err(e)
        _dev_event("models", results=len(models), error=None)
        return {"object": "list", "data": models, "ai_available": bool(models)}

    @router.get("/settings", response_model=SettingsResponse)
    def get_settings() -> dict:
        """Current [ai] config for the settings dialog; api_key redacted."""
        from ..config import config_path, load_config

        try:
            cfg = load_config()
        except ValueError as e:
            raise HTTPException(status_code=500, detail=f"bad config: {e}")
        if cfg is None:
            return {"configured": False, "config_path": str(config_path())}
        return {
            "configured": True,
            "config_path": str(config_path()),
            "ai": {
                "provider": cfg.provider,
                "model": cfg.model,
                "api_key_set": bool(cfg.api_key or cfg.api_key_env),
                "api_key_env": cfg.api_key_env,
                "base_url": cfg.base_url,
                "enabled": cfg.enabled,
            },
        }

    @router.put("/settings", response_model=SettingsPutResponse)
    def put_settings(payload: SettingsPutRequest) -> dict:
        """Write the [ai] config section. api_key left untouched when omitted."""
        from ..config import VALID_PROVIDERS, AIConfig, load_config, save_config

        ai = payload.ai
        try:
            existing = load_config()
        except ValueError as e:
            raise HTTPException(status_code=500, detail=f"bad config: {e}")

        provider = (ai.provider or "").strip()
        model = (ai.model or "").strip()
        if not provider or not model:
            raise HTTPException(status_code=422, detail="provider and model required")
        # {env.*} templates from the loaded config; a field keeps its template
        # (so the secret is never flattened to plaintext) unless the PUT
        # explicitly provides a new literal value for it.
        templates = dict(existing.env_templates) if existing is not None else {}
        api_key = ai.api_key
        api_key_env = (ai.api_key_env or "").strip() or None
        if api_key is not None and not str(api_key).strip():
            api_key = None
        if api_key is None:
            templates.pop("api_key", None)
            if existing is not None and not api_key_env:
                api_key = existing.api_key  # keep stored key when dialog omits it
                if existing.env_templates.get("api_key"):
                    templates["api_key"] = existing.env_templates["api_key"]
        else:
            templates.pop("api_key", None)
        if api_key_env is None:
            templates.pop("api_key_env", None)
            if existing is not None and existing.env_templates.get("api_key_env"):
                api_key_env = existing.api_key_env
                templates["api_key_env"] = existing.env_templates["api_key_env"]
        else:
            templates.pop("api_key_env", None)
        base_url = (ai.base_url or "").strip() or None
        if base_url is None:
            templates.pop("base_url", None)
            if existing is not None and (
                existing.env_templates.get("base_url") or existing.base_url
            ):
                # keep the stored base_url when the PUT omits it (same as api_key)
                base_url = existing.base_url
                if existing.env_templates.get("base_url"):
                    templates["base_url"] = existing.env_templates["base_url"]
        else:
            templates.pop("base_url", None)
        cfg = AIConfig(
            provider=provider,
            model=model,
            api_key=str(api_key).strip() if api_key else None,
            api_key_env=api_key_env,
            base_url=base_url,
            enabled=bool(ai.enabled),
            env_templates=templates,
        )
        if cfg.provider not in VALID_PROVIDERS:
            raise HTTPException(
                status_code=422,
                detail=f"provider must be one of {', '.join(sorted(VALID_PROVIDERS))}",
            )
        try:
            path = save_config(cfg)
        except OSError as e:
            raise HTTPException(status_code=500, detail=f"cannot write config: {e}")
        log.info("UI: config saved to %s", path)
        return SettingsPutResponse(ok=True, config_path=str(path)).model_dump()

    @router.post("/settings/test", response_model=SettingsTestResponse)
    def test_settings(payload: SettingsTestRequest) -> dict:
        """Verify provider + api_key + base_url + model with a minimal call.

        Body: {"ai": {provider, model, base_url?, api_key?}}; blank/omitted
        api_key falls back to the stored key (same as PUT /settings).
        Returns {ok: bool, detail: str}. Lazy imports, ~10s timeout.
        """
        from .. import ai as ai_mod
        from ..config import AIConfig, load_config

        ai = payload.ai
        provider = (ai.provider or "").strip()
        model = (ai.model or "").strip()
        if not provider or not model:
            raise HTTPException(status_code=422, detail="provider and model required")
        api_key = ai.api_key
        api_key = str(api_key).strip() if api_key else None
        base_url = (ai.base_url or "").strip() or None
        if api_key is None or base_url is None:
            try:
                existing = load_config()
            except ValueError:
                existing = None
            if existing is not None:
                base_url = base_url or existing.base_url
                api_key = api_key if api_key is not None else existing.api_key
        cfg = AIConfig(provider=provider, model=model, api_key=api_key, base_url=base_url)
        try:
            result = _run_async(lambda: ai_mod.test_provider_connection(cfg))
            result = {"ok": bool(result["ok"]), "detail": str(result["detail"])}
        except Exception as e:  # pragma: no cover - defensive
            raise HTTPException(status_code=500, detail=f"test failed: {e}")
        _dev_event(
            "settings_test",
            q=f"{provider}:{model}",
            error=None if result["ok"] else result["detail"][:80],
        )
        return result

    return router
