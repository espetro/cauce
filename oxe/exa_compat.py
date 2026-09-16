import hashlib
import logging
import re
import time
import uuid
from typing import Any
from urllib.parse import urlparse

from ddgs import DDGS
from ddgs.exceptions import RatelimitException, TimeoutException

log = logging.getLogger(__name__)

_SENTENCE_SPLIT = re.compile(r"(?<=[.!?])\s+")
_NUM_CLAMP = (1, 30)
_FAVICON = "https://www.google.com/s2/favicons?domain={netloc}&sz=32"
# Pause before the single page-fetch retry (DDG html rate limiting).
_PAGE_RETRY_DELAY_S = 1.5


def cache_key(req: dict) -> str:
    contents = req.get("contents") or {}
    backend = req.get("_backend") or "ddg"
    norm = [
        backend,
        (req.get("query") or "").lower().strip(),
        max(_NUM_CLAMP[0], min(_NUM_CLAMP[1], int(req.get("numResults") or 10))),
        req.get("type") or "auto",
        tuple(sorted(req.get("includeDomains") or [])),
        tuple(sorted(req.get("excludeDomains") or [])),
        bool(contents.get("highlights")),
        bool(contents.get("text")),
    ]
    # Page is part of cache identity only when it differs from the default, so
    # existing keys (page absent or 1) stay valid.
    page = int(req.get("page") or 1)
    if page > 1:
        norm.append(("page", page))
    return hashlib.sha256(repr(tuple(norm)).encode("utf-8")).hexdigest()


def build_query(req: dict) -> str:
    parts: list[str] = [(req.get("query") or "").strip()]
    for d in req.get("includeDomains") or []:
        if d:
            parts.append(f"site:{d}")
    for d in req.get("excludeDomains") or []:
        if d:
            parts.append(f"-site:{d}")
    return " ".join(p for p in parts if p)


def _extract_highlights(body: str | None, max_n: int = 3) -> list[str]:
    if not body:
        return []
    sentences = [s.strip() for s in _SENTENCE_SPLIT.split(body) if s.strip()]
    return sentences[:max_n]


def _favicon_for(url: str) -> str:
    try:
        netloc = urlparse(url).netloc
    except ValueError:
        netloc = ""
    return _FAVICON.format(netloc=netloc)


def _dgr_to_exa(r: dict, contents_highlights: bool, contents_text: bool) -> dict:
    href = r.get("href") or ""
    body = r.get("body") or ""
    text = body if contents_text else ""
    highlights = _extract_highlights(body) if contents_highlights else []
    return {
        "title": r.get("title") or "",
        "url": href,
        "id": href,
        "text": text,
        "highlights": highlights,
        "highlightScores": [0.5] * len(highlights),
        "publishedDate": None,
        "author": None,
        "image": None,
        "favicon": _favicon_for(href),
        "extras": {"links": []},
    }


def search(req: dict, engine: str | None = None) -> dict:
    ignored = []
    for field in ("startPublishedDate", "endPublishedDate", "additionalQueries",
                  "systemPrompt", "outputSchema", "stream"):
        if req.get(field):
            ignored.append(field)
    if (req.get("contents") or {}).get("summary"):
        ignored.append("contents.summary")
    if ignored:
        log.warning("exa_compat: ignoring unsupported fields: %s", ignored)

    contents = req.get("contents") or {}
    contents_highlights = bool(contents.get("highlights"))
    contents_text = bool(contents.get("text"))

    num_results = max(_NUM_CLAMP[0], min(_NUM_CLAMP[1], int(req.get("numResults") or 10)))
    page = max(1, int(req.get("page") or 1))
    query = build_query(req)
    search_type = req.get("type") or "auto"

    region = "wt-wt" if search_type == "instant" else None
    timelimit = "d" if req.get("category") == "news" else None

    # Explicit engine (from DdgsBackend) wins; otherwise DDG with auto fallback.
    if engine:
        backends_to_try = [engine]
    else:
        backends_to_try = ["duckduckgo", "auto"]

    raw: list[dict[str, Any]] = []
    last_err: Exception | None = None
    for backend in backends_to_try:
        try:
            kwargs: dict[str, Any] = dict(
                query=query,
                max_results=num_results,
                backend=backend,
                safesearch="moderate",
            )
            if region:
                kwargs["region"] = region
            if timelimit:
                kwargs["timelimit"] = timelimit
            if page > 1:
                kwargs["page"] = page
            try:
                raw = list(DDGS().text(**kwargs))
            except Exception as e:
                # DDG html paging is aggressively rate limited; a single
                # immediate retry recovers most transient "No results found."
                log.warning(
                    "exa_compat: backend %s page %s failed (%s), retrying once",
                    backend, page, e,
                )
                time.sleep(_PAGE_RETRY_DELAY_S)
                raw = list(DDGS().text(**kwargs))
            if raw:
                break
        except Exception as e:
            last_err = e
            log.warning("exa_compat: backend %s failed: %s", backend, e)
            continue

    results = [_dgr_to_exa(r, contents_highlights, contents_text) for r in raw]
    payload = {
        "requestId": str(uuid.uuid4()),
        "searchType": search_type,
        "results": results,
        "_page": page,
        "costDollars": {"total": 0.0},
    }
    if not raw and last_err is not None:
        log.error("exa_compat: all backends failed, last error: %s", last_err)
        if isinstance(last_err, RatelimitException):
            kind = "rate_limited"
        elif isinstance(last_err, TimeoutException):
            kind = "timeout"
        else:
            kind = "backend_error"
        payload["_error"] = str(last_err)
        payload["_error_kind"] = kind
    return payload
