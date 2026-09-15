import logging
import os
import time
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any, Callable, Optional
from urllib.parse import quote

from fastapi import FastAPI, Form, Header, HTTPException, Query, Request
from fastapi.responses import (
    FileResponse,
    HTMLResponse,
    JSONResponse,
    RedirectResponse,
    Response,
    StreamingResponse,
)
from mcp.server.transport_security import TransportSecuritySettings
from pydantic import BaseModel

from . import __version__, exa_compat
from .cache import TTLCache
from .devlog import DEV as _DEV
from .devlog import event as _dev_event
from .search import do_search

log = logging.getLogger(__name__)

PORT = int(os.getenv("OXE_PORT", "4479"))
CACHE_DIR = os.getenv("OXE_CACHE_DIR", os.path.expanduser("~/.cache/oxe"))
LOG_LEVEL = os.getenv("OXE_LOG_LEVEL", "INFO").upper()

logging.basicConfig(
    level=getattr(logging, LOG_LEVEL, logging.INFO),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)

VERSION = __version__
SERVICE_NAME = "oxe"
SEARCH_LOG_RETENTION_DAYS = int(os.getenv("OXE_SEARCH_LOG_RETENTION_DAYS", "30"))


class ContentsModel(BaseModel):
    text: Any = None
    highlights: Any = None
    summary: Any = None

    model_config = {"extra": "allow"}


class ExaRequest(BaseModel):
    query: str
    type: Optional[str] = "auto"
    numResults: Optional[int] = 10
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


# module-level cache kept for mcp_server wiring and backward compat
cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))
log.info("cache initialized at %s/cache.db", CACHE_DIR)

_NO_UI_PAGE = """<!doctype html><html><head><title>oxe</title></head>
<body style="font-family:system-ui,sans-serif;max-width:36rem;margin:4rem auto;padding:0 1rem">
<h1>oxe web UI not built</h1>
<p>The SPA bundle is not present. Options:</p>
<ul>
<li>Dev checkout: run <code>mise run build:ui</code> (builds <code>ui/dist</code>),
   or set <code>OXE_UI_DIST</code>.</li>
<li>Installed package: set <code>OXE_UI_DIST</code> to a directory containing
   <code>index.html</code>.</li>
</ul>
<p>The JSON API (<code>POST /search</code>) and MCP (<code>/mcp/</code>) work without the UI.</p>
</body></html>"""


def _ui_dist_dir() -> Path | None:
    """Locate the built web UI (Preact SPA), if present.

    Order: $OXE_UI_DIST, ./ui/dist (repo checkout), packaged oxe/ui_dist.
    """
    env = os.getenv("OXE_UI_DIST", "").strip()
    if env:
        p = Path(env).expanduser()
        return p if (p / "index.html").is_file() else None
    repo = Path("ui/dist")
    if (repo / "index.html").is_file():
        return repo
    pkg = Path(__file__).parent / "ui_dist"
    if (pkg / "index.html").is_file():
        return pkg
    return None


_MEDIA_TYPES = {
    ".html": "text/html",
    ".js": "application/javascript",
    ".css": "text/css",
    ".json": "application/json",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ico": "image/x-icon",
    ".woff": "font/woff",
    ".woff2": "font/woff2",
    ".map": "application/json",
    ".txt": "text/plain",
    ".webmanifest": "application/manifest+json",
}
CLICK_RETENTION_DAYS = int(os.getenv("OXE_CLICK_RETENTION_DAYS", "30"))


def _http_search_logger(c: TTLCache) -> Callable[[dict], None]:
    """Default observer for make_app: writes search_log rows, client='http'."""

    def _log(payload: dict) -> None:
        q = (payload.get("_q") or "").strip()
        if not q:
            # network rows miss _q; backfill from the same hash's cache/log text
            q = c.lookup_query_text(payload.get("_q_hash") or "") or ""
        c.log_search(
            query_text=q[:200],
            query_hash=payload.get("_q_hash") or "",
            source=payload.get("_source") or "network",
            backend=payload.get("_backend", "ddg"),
            result_count=len(payload.get("results") or []),
            duration_ms=payload.get("_duration_ms"),
            client="http",
        )

    return _log


def make_app(
    cache: TTLCache | None = None,
    backend: object | None = None,
    on_result: Callable[[dict], None] | None = None,
) -> FastAPI:
    """Build the oxe FastAPI app. All handlers close over the given cache.

    cache defaults to the module-level instance; on_result defaults to a
    search_log writer with client='http'.
    """
    c = cache or globals()["cache"]
    if backend is None:
        from .registry import build_from_env

        backend = build_from_env()
    observer = on_result or _http_search_logger(c)

    try:
        from .mcp_server import mcp, set_cache

        set_cache(c)
        mcp_app = mcp.streamable_http_app(
            streamable_http_path="/",
            transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
        )

        @asynccontextmanager
        async def lifespan(app: FastAPI):
            async with mcp_app.router.lifespan_context(mcp_app):
                yield

        app = FastAPI(title=SERVICE_NAME, version=VERSION, lifespan=lifespan)
        app.mount("/mcp", mcp_app)
        log.info("MCP route mounted at /mcp (DNS rebinding protection disabled)")
    except Exception as e:
        log.exception("failed to mount MCP sub-app: %s", e)
        app = FastAPI(title=SERVICE_NAME, version=VERSION)

    @app.on_event("startup")
    def _prune_on_startup() -> None:
        n = c.prune_clicks(CLICK_RETENTION_DAYS)
        if n:
            log.info("startup: pruned %d clicks older than %d days", n, CLICK_RETENTION_DAYS)
        m = c.prune_search_log(SEARCH_LOG_RETENTION_DAYS)
        if m:
            log.info(
                "startup: pruned %d search_log rows older than %d days",
                m,
                SEARCH_LOG_RETENTION_DAYS,
            )

    @app.get("/health")
    def health() -> dict:
        return {
            "status": "ok",
            "service": SERVICE_NAME,
            "cache_size": c.stats()["rows"],
            "version": VERSION,
            "pid": os.getpid(),
        }

    @app.post("/search")
    def exa_search(
        req: ExaRequest,
        authorization: Optional[str] = Header(default=None),
        x_api_key: Optional[str] = Header(default=None, alias="x-api-key"),
    ) -> dict:
        if authorization or x_api_key:
            log.info("/search: auth header present (ignored)")
        req_dict = req.model_dump(exclude_none=True)
        if req_dict.get("numResults") is None:
            req_dict["numResults"] = 10
        refresh = bool(req_dict.pop("_refresh", False))
        if refresh:
            key = exa_compat.cache_key(req_dict | {"_backend": getattr(backend, "name", "ddg")})
            c.delete(key)
        _t0 = time.monotonic() if _DEV else None
        try:
            out = do_search(c, req_dict, backend=backend, on_result=observer)
        except Exception as e:
            _dev_event(
                "search",
                q=req_dict.get("query", ""),
                page=req_dict.get("page", 1),
                error=str(e)[:200],
            )
            raise
        if _DEV:
            _dev_event(
                "search",
                q=req_dict.get("query", ""),
                page=req_dict.get("page", 1),
                source=out.get("_source"),
                duration_ms=out.get("_duration_ms")
                if out.get("_duration_ms") is not None
                else int((time.monotonic() - _t0) * 1000),
                results=len(out.get("results") or []),
            )
        return out

    def _search_payload(q: str, num_results: int = 10, page: int = 1) -> dict:
        return do_search(
            c,
            {
                "query": q,
                "numResults": num_results,
                "page": page,
                "contents": {"text": True, "highlights": True},
            },
            backend=backend,
            on_result=observer,
        )

    @app.get("/search", response_class=HTMLResponse)
    def ui_search_get(
        q: Optional[str] = Query(default=None),
        p: int = Query(default=1, ge=1),
        request: Request = None,
        accept: Optional[str] = Header(default=None),
    ) -> Response:
        if not q or not q.strip():
            return RedirectResponse(url="/", status_code=302)
        q = q.strip()
        if accept and "application/json" in accept:
            return JSONResponse(_search_payload(q, page=p))
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(dist / "index.html", media_type="text/html")
        return HTMLResponse(_NO_UI_PAGE)

    @app.get("/suggest")
    def suggest(q: str = Query(...)) -> list:
        """OpenSearch Suggestions JSON: ["prefix", ["s1", "s2"], ...]."""
        prefix = q.strip()
        if not prefix:
            return [prefix, [], [], []]
        out = [prefix, c.suggest_queries(prefix, limit=3), [], []]
        _dev_event("suggest", q=prefix, results=len(out[1]))
        return out

    @app.get("/ac")
    def ac(q: str = Query(...)) -> list[str]:
        """Proxy DuckDuckGo autocomplete (browser CORS blocks direct calls)."""
        prefix = q.strip()
        if not prefix:
            return []
        try:
            import json as _json
            from urllib.request import Request, urlopen

            url = "https://duckduckgo.com/ac/?type=list&q=" + quote(prefix)
            req = Request(url, headers={"User-Agent": "oxe/autocomplete"})
            with urlopen(req, timeout=3) as resp:
                data = _json.loads(resp.read())
            phrases = data[1] if isinstance(data, list) and len(data) > 1 else []
            out = [str(p) for p in phrases][:6]
            _dev_event("ac", q=prefix, results=len(out))
            return out
        except Exception as e:
            log.warning("/ac: ddg autocomplete failed: %s", e)
            return []

    @app.get("/cache/stats")
    def cache_stats() -> dict:
        return c.stats()

    @app.post("/cache/invalidate")
    def cache_invalidate(
        authorization: Optional[str] = Header(default=None),
        x_api_key: Optional[str] = Header(default=None, alias="x-api-key"),
    ) -> dict:
        actor = "anonymous"
        if authorization or x_api_key:
            actor = (authorization or x_api_key or "").split()[-1][:12]
        log.warning("/cache/invalidate called by %s", actor)
        deleted = c.invalidate()
        return {"deleted": deleted}

    @app.get("/", response_class=HTMLResponse)
    def ui_search(q: Optional[str] = Query(default=None)) -> Response:
        if not q or not q.strip():
            dist = _ui_dist_dir()
            if dist is not None:
                return FileResponse(dist / "index.html", media_type="text/html")
            return HTMLResponse(_NO_UI_PAGE)
        return RedirectResponse(url=f"/search?q={quote(q)}", status_code=302)

    @app.get("/assets/{name}")
    def ui_assets(name: str) -> Response:
        """Serve built SPA assets from the UI dist dir (404 when absent)."""
        if "/" in name or ".." in name:
            raise HTTPException(status_code=400, detail="bad path")
        dist = _ui_dist_dir()
        p = dist / "assets" / name if dist else None
        if p is None or not p.is_file():
            raise HTTPException(status_code=404, detail="not found")
        media = _MEDIA_TYPES.get(p.suffix, "application/octet-stream")
        return FileResponse(p, media_type=media)

    @app.get("/history", response_class=HTMLResponse)
    def ui_history(
        q: Optional[str] = Query(default=None),
        since: Optional[str] = Query(default=None),
    ) -> Response:
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(dist / "index.html", media_type="text/html")
        return HTMLResponse(_NO_UI_PAGE)

    @app.post("/history/delete")
    def ui_history_delete(scope: str = Form(...)) -> RedirectResponse:
        n = c.delete_clicks(scope)
        log.info("UI: deleted %d clicks (scope=%s)", n, scope)
        return RedirectResponse(url="/history", status_code=303)

    @app.post("/answer")
    def ai_answer(req: dict) -> Response:
        """SSE-stream an AI answer for a query. Requires a configured model."""
        from . import ai as ai_mod
        from .config import load_config

        query = ((req or {}).get("query") or "").strip() if isinstance(req, dict) else ""
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
                    yield ai_mod.sse_format(
                        {"type": "sources", "sources": payload["sources"]}
                    )
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
                query, cfg, cache=c, backend=backend, on_result=observer
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

    @app.get("/v1/models")
    def list_models() -> dict:
        """Provider model listing via direct SDK calls; [] when unconfigured."""
        from .config import load_config

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

                client = AsyncOpenAI(api_key=api_key, base_url=cfg.base_url)
                resp = _run_async(client.models.list)
                models = [
                    {"id": m.id, "object": "model", "owned_by": getattr(m, "owned_by", None)}
                    for m in resp.data
                ]
            elif cfg.provider == "anthropic":
                from anthropic import AsyncAnthropic  # lazy

                client = AsyncAnthropic(api_key=api_key, base_url=cfg.base_url)
                resp = _run_async(client.models.list)
                models = [{"id": m.id, "object": "model"} for m in resp.data]
        except ImportError:
            log.warning("/v1/models: provider SDK not installed (pip install oxe[ai])")
            _dev_event("models", results=0, error="sdk not installed")
        except Exception as e:
            log.warning("/v1/models: provider listing failed: %s", e)
            _dev_event("models", results=0, error=str(e)[:200])
            return _err(e)
        _dev_event("models", results=len(models), error=None)
        return {"object": "list", "data": models, "ai_available": bool(models)}

    @app.get("/settings")
    def get_settings() -> dict:
        """Current [ai] config for the settings dialog; api_key redacted."""
        from .config import config_path, load_config

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

    @app.put("/settings")
    def put_settings(payload: dict) -> dict:
        """Write the [ai] config section. api_key left untouched when omitted."""
        from .config import VALID_PROVIDERS, AIConfig, load_config, save_config

        if not isinstance(payload or {}, dict):
            raise HTTPException(status_code=422, detail="json body required")
        ai = (payload or {}).get("ai")
        if not isinstance(ai, dict):
            raise HTTPException(status_code=422, detail="ai object required")
        try:
            existing = load_config()
        except ValueError as e:
            raise HTTPException(status_code=500, detail=f"bad config: {e}")

        provider = (ai.get("provider") or "").strip()
        model = (ai.get("model") or "").strip()
        if not provider or not model:
            raise HTTPException(status_code=422, detail="provider and model required")
        # {env.*} templates from the loaded config; a field keeps its template
        # (so the secret is never flattened to plaintext) unless the PUT
        # explicitly provides a new literal value for it.
        templates = dict(existing.env_templates) if existing is not None else {}
        api_key = ai.get("api_key")
        api_key_env = (ai.get("api_key_env") or "").strip() or None
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
        base_url = (ai.get("base_url") or "").strip() or None
        if base_url is None:
            templates.pop("base_url", None)
            if existing is not None and existing.env_templates.get("base_url"):
                base_url = existing.base_url
                templates["base_url"] = existing.env_templates["base_url"]
        else:
            templates.pop("base_url", None)
        cfg = AIConfig(
            provider=provider,
            model=model,
            api_key=str(api_key).strip() if api_key else None,
            api_key_env=api_key_env,
            base_url=base_url,
            enabled=bool(ai.get("enabled", True)),
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
        return {"ok": True, "config_path": str(path)}

    @app.post("/settings/test")
    def test_settings(payload: dict) -> dict:
        """Verify provider + api_key + base_url + model with a minimal call.

        Body: {"ai": {provider, model, base_url?, api_key?}}; blank/omitted
        api_key falls back to the stored key (same as PUT /settings).
        Returns {ok: bool, detail: str}. Lazy imports, ~10s timeout.
        """
        from . import ai as ai_mod
        from .config import AIConfig, load_config

        ai = (payload or {}).get("ai")
        if not isinstance(ai, dict):
            raise HTTPException(status_code=422, detail="ai object required")
        provider = (ai.get("provider") or "").strip()
        model = (ai.get("model") or "").strip()
        if not provider or not model:
            raise HTTPException(status_code=422, detail="provider and model required")
        api_key = ai.get("api_key")
        api_key = str(api_key).strip() if api_key else None
        base_url = (ai.get("base_url") or "").strip() or None
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

    @app.post("/click")
    def ui_click(payload: dict) -> dict:
        qh = (payload.get("query_hash") or "").strip()
        rid = (payload.get("result_id") or "").strip()
        url = (payload.get("url") or "").strip()
        title = (payload.get("title") or "").strip()[:500]
        source = (payload.get("source") or "web").strip()[:16]
        if not (qh and rid and url):
            raise HTTPException(status_code=422, detail="query_hash, result_id, url required")
        click_id = c.record_click(qh, rid, url, title, source=source)
        return {"ok": True, "click_id": click_id}

    @app.get("/cache", response_class=HTMLResponse)
    def ui_cache(
        q: Optional[str] = Query(default=None),
        include_expired: bool = Query(default=False),
        page: int = Query(default=0, ge=0),
    ) -> str:
        return HTMLResponse(_NO_UI_PAGE)

    @app.get("/row/{key}")
    def ui_row(key: str) -> Response:
        qtext = c.lookup_query_text(key)
        if not qtext:
            raise HTTPException(status_code=404, detail="row not found")
        return RedirectResponse(url=f"/search?q={quote(qtext)}", status_code=302)

    @app.post("/row/{key}/delete")
    def ui_row_delete(key: str, request: Request) -> Response:
        if not c.delete(key):
            # already gone: treat as success so idempotent UI refreshes stay clean
            if "text/html" not in request.headers.get("accept", "text/html"):
                return Response(status_code=204)
        log.info("UI: deleted cache row %s", key[:12])
        return RedirectResponse(url="/cache", status_code=303)

    return app


def _share_info(c: TTLCache, q: str, payload: dict | None) -> dict | None:
    """Build Share row data for the search UI: query, count, source, age, ttl."""
    if payload is None:
        return None
    count = len(payload.get("results") or [])
    source = payload.get("_source") or "network"
    qh = payload.get("_q_hash") or ""
    age_s = None
    ttl_left = None
    if qh:
        rows = c.list_rows(q=q, limit=5)
        row = next((r for r in rows if r["hash"] == qh), None)
        if row is None and rows:
            row = rows[0]
        if row is not None:
            now = int(time.time())
            if row["expired"]:
                ttl_left = 0
            else:
                total = max(row["expires_at"] - now, 0)
                ttl_left = total
    return {
        "query": q,
        "result_count": count,
        "source": source,
        "query_hash": qh,
        "age_s": age_s,
        "ttl_left_s": ttl_left,
    }


# module-level app for `uvicorn oxe.server:app` backward compat
app = make_app()
