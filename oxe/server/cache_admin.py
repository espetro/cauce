import logging
import time
from typing import Literal, Optional
from urllib.parse import quote

from fastapi import APIRouter, Header, HTTPException, Query, Request
from fastapi.responses import (
    FileResponse,
    HTMLResponse,
    RedirectResponse,
    Response,
)

from .. import stats as stats_mod
from ..cache import TTLCache
from .schemas import (
    ApiHistoryResponse,
    ApiStatsResponse,
    CacheInvalidateResponse,
    CacheStatsResponse,
    ClickRequest,
    ClickResponse,
    HistoryDeleteResponse,
)
from .state import AppState
from .ui_dist import _NO_UI_PAGE, _ui_dist_dir

log = logging.getLogger(__name__)

_SINCE_HOURS = {"24h": 24, "7d": 168, "30d": 720, "all": None}


def build_router(state: AppState) -> APIRouter:
    router = APIRouter()
    c = state.cache

    @router.get("/cache/stats", response_model=CacheStatsResponse)
    def cache_stats() -> dict:
        return c.stats()

    @router.post("/cache/invalidate", response_model=CacheInvalidateResponse)
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

    @router.get("/api/history", response_model=ApiHistoryResponse)
    def api_history(
        since: Literal["24h", "7d", "30d", "all"] = Query(default="all"),
        q: Optional[str] = Query(default=None),
        limit: int = Query(default=50, ge=1, le=200),
        kind: Literal["all", "clicks", "cache"] = Query(default="all"),
    ) -> dict:
        """JSON API for the SPA /history page: merged user activity view.

        Params: since=24h|7d|30d|all (default all), q=substring filter,
        limit=1..200 (default 50), kind=all|clicks|cache (default all).
        Returns {items, clicks, cache_rows, limit, since} sorted newest
        first; each item has a `kind` field ("click" | "cache").
        """
        hours = _SINCE_HOURS[since]
        items: list[dict] = []
        if kind in ("all", "clicks"):
            for r in c.get_clicks(query_text=q, limit=200, since_hours=hours):
                items.append(
                    {
                        "kind": "click",
                        "clicked_at": r["clicked_at"],
                        "sort_at": r["clicked_at"],
                        "query_hash": r["query_hash"],
                        "query": r["query"],
                        "result_id": r["result_id"],
                        "url": r["url"],
                        "title": r["title"],
                        "source": r["source"],
                    }
                )
        if kind in ("all", "cache"):
            for r in c.list_rows(q=q, limit=200):
                created = r.get("created_at") or r["expires_at"]
                if hours is not None and created < time.time() - hours * 3600:
                    continue
                items.append(
                    {
                        "kind": "cache",
                        "created_at": created,
                        "sort_at": created,
                        "query_hash": r["hash"],
                        "query": r["query"],
                        "expires_at": r["expires_at"],
                        "hits": r["hits"],
                        "size_bytes": r["size_bytes"],
                    }
                )
        items.sort(key=lambda i: i.pop("sort_at"), reverse=True)
        n_clicks = sum(1 for i in items if i["kind"] == "click")
        return {
            "items": items[:limit],
            "clicks": n_clicks,
            "cache_rows": len(items) - n_clicks,
            "limit": limit,
            "since": since,
        }

    @router.get("/api/stats", response_model=ApiStatsResponse)
    def api_stats(days: int = Query(default=14, ge=1, le=90)) -> dict:
        """JSON API for the SPA /dashboard page: aggregates from search_log
        (per-day, hit rate, latency percentiles, top/zero-result queries,
        client split) plus cache stats. Params: days=1..90 (default 14)."""
        data = stats_mod.build_json(c.db_path, days=days)
        data["cache"] = c.stats()
        return data

    @router.get("/history", response_class=HTMLResponse)
    def ui_history(
        q: Optional[str] = Query(default=None),
        since: Optional[str] = Query(default=None),
    ) -> Response:
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(dist / "index.html", media_type="text/html")
        return HTMLResponse(_NO_UI_PAGE)

    @router.post("/history/delete")
    async def ui_history_delete(request: Request) -> Response:
        """Delete click history. Accepts form field `scope` (HTML POST,
        redirects back to /history) or JSON body {"scope": ...} (API clients,
        JSON reply {"ok": true, "deleted": n})."""
        ctype = request.headers.get("content-type", "")
        if "application/json" in ctype:
            raw = await request.body()
            body = await request.json() if raw else {}
            scope = (body or {}).get("scope", "")
            api = True
        else:
            form = await request.form()
            scope = form.get("scope", "")
            api = False
        scope = (scope or "").strip()
        if scope not in ("24h", "7d", "30d", "all"):
            raise HTTPException(status_code=422, detail="scope must be 24h|7d|30d|all")
        n = c.delete_clicks(scope)
        log.info("UI: deleted %d clicks (scope=%s)", n, scope)
        if api:
            return HistoryDeleteResponse(ok=True, deleted=n).model_dump()
        return RedirectResponse(url="/history", status_code=303)

    @router.get("/dashboard", response_class=HTMLResponse)
    def ui_dashboard() -> Response:
        """Serve the SPA shell for the /dashboard route (data via /api/stats)."""
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(dist / "index.html", media_type="text/html")
        return HTMLResponse(_NO_UI_PAGE)

    @router.post("/click", response_model=ClickResponse)
    def ui_click(payload: ClickRequest) -> dict:
        qh = payload.query_hash.strip()
        rid = payload.result_id.strip()
        url = payload.url.strip()
        title = payload.title.strip()[:500]
        source = payload.source
        if not (qh and rid and url):
            raise HTTPException(status_code=422, detail="query_hash, result_id, url required")
        click_id = c.record_click(qh, rid, url, title, source=source)
        return ClickResponse(ok=True, click_id=click_id).model_dump()

    @router.get("/cache", response_class=HTMLResponse)
    def ui_cache(
        q: Optional[str] = Query(default=None),
        include_expired: bool = Query(default=False),
        page: int = Query(default=0, ge=0),
    ) -> Response:
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(dist / "index.html", media_type="text/html")
        return HTMLResponse(_NO_UI_PAGE)

    @router.get("/row/{key}")
    def ui_row(key: str) -> Response:
        qtext = c.lookup_query_text(key)
        if not qtext:
            raise HTTPException(status_code=404, detail="row not found")
        return RedirectResponse(url=f"/search?q={quote(qtext)}", status_code=302)

    @router.post("/row/{key}/delete")
    def ui_row_delete(key: str, request: Request) -> Response:
        if not c.delete(key):
            # already gone: treat as success so idempotent UI refreshes stay clean
            if "text/html" not in request.headers.get("accept", "text/html"):
                return Response(status_code=204)
        log.info("UI: deleted cache row %s", key[:12])
        return RedirectResponse(url="/cache", status_code=303)

    return router


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
