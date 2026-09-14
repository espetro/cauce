import html
import json
import time
from string import Template
from typing import Any

from .cache import TTLCache
from importlib.resources import files as _pkg_files


def _tpl(name: str) -> Template:
    return Template(_pkg_files("oxe.static").joinpath(name).read_text(encoding="utf-8"))


PAGE = 50
ASSET_VERSION = "3"


def _fmt_ts(epoch: int | None) -> str:
    if not epoch:
        return "—"
    return time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(epoch))


def _fmt_bytes(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    if n < 1024 * 1024:
        return f"{n / 1024:.1f} KB"
    return f"{n / (1024 * 1024):.2f} MB"


def _fmt_remaining(expires_at: int, now: int) -> str:
    delta = expires_at - now
    if delta <= 0:
        return "expired"
    if delta < 60:
        return f"{delta}s"
    if delta < 3600:
        return f"{delta // 60}m"
    if delta < 86400:
        return f"{delta // 3600}h {delta % 3600 // 60}m"
    return f"{delta // 86400}d {delta % 86400 // 3600}h"


def _esc(s: Any, attr: bool = False) -> str:
    if s is None:
        return ""
    out = html.escape(str(s))
    if attr:
        out = out.replace("'", "&#39;")
    return out


def _first_preview(text: str | None, n: int = 280) -> str:
    if not text:
        return ""
    text = " ".join(text.split())
    if len(text) <= n:
        return text
    return text[: n - 1] + "…"


_BASE_CSS = "/static/ui.css?v=" + ASSET_VERSION
_BASE_JS = "/static/app.js?v=" + ASSET_VERSION


_SHELL = _tpl("shell.html")


_INDEX = _tpl("index.html")


_SEARCH = _tpl("search.html")


_HISTORY = _tpl("history.html")


_ROW = _tpl("row.html")


def _render_index(
    cache: TTLCache,
    q: str | None,
    include_expired: bool,
    page: int,
) -> tuple[str, str]:
    s = cache.stats()
    total_rows = s["rows"] if include_expired else s["unexpired_rows"]
    rows = cache.list_rows(q=q, include_expired=include_expired, limit=PAGE, offset=page * PAGE)
    now = int(time.time())

    stats_line = (
        f"{s['unexpired_rows']} live · {s['rows']} total · "
        f"{_fmt_bytes(s['db_size_bytes'])} · {s['total_hits']} cache hits"
    )

    if not rows:
        rows_html = '<tr><td colspan="7" class="empty">no rows match this filter.</td></tr>'
    else:
        rendered = []
        for r in rows:
            rendered.append(
                "<tr>"
                f"<td><a href='/row/{_esc(r['hash'])}'>{_esc(r['query'] or '(no query)')}</a></td>"
                f"<td class='num'>{r['hits']}</td>"
                f"<td class='num'>{'?'}</td>"
                f"<td class='num'>{_fmt_bytes(r['size_bytes'])}</td>"
                f"<td class='num'>{_esc(_fmt_remaining(r['expires_at'], now))}</td>"
                f"<td class='ts'>{_esc(_fmt_ts(r['expires_at']))}</td>"
                f"<td class='hash'>{_esc(r['hash'][:12])}…</td>"
                "</tr>"
            )
        rows_html = "\n".join(rendered)

    pager_bits = []
    if page > 0:
        qp = _qs(q, include_expired, page - 1)
        pager_bits.append(f"<a href='/cache?{qp}'>← prev</a>")
    if len(rows) == PAGE:
        qp = _qs(q, include_expired, page + 1)
        pager_bits.append(f"<a href='/cache?{qp}'>next →</a>")
    pager_html = " ".join(pager_bits) if pager_bits else f"<span class='muted'>page {page + 1} · {total_rows} total rows</span>"

    clear_link = "<a href='/cache' class='clear'>clear</a>" if (q or include_expired) else ""
    body = _INDEX.substitute(
        stats_line=stats_line,
        q_esc=_esc(q or ""),
        exp_check="checked" if include_expired else "",
        rows_html=rows_html,
        pager_html=pager_html,
        clear_link=clear_link,
    )
    return "Cache", body


_CARD = _tpl("card.html")


def _domain_of(url: str) -> str:
    try:
        from urllib.parse import urlsplit

        return urlsplit(url).netloc.removeprefix("www.") or url
    except Exception:
        return url


def _render_cards(payload: dict) -> tuple[str, str]:
    """Render search results server-side, mirroring the result markup the JS builds."""
    results = payload.get("results") or []
    if not results:
        return "", ""
    cards = []
    qh = payload.get("_q_hash") or ""
    for r in results:
        title = r.get("title") or "(untitled)"
        url = r.get("url") or ""
        domain = _domain_of(url)
        snippet = _first_preview(r.get("text"), 200)
        preview = _esc(((r.get("text") or "").strip())[:400])
        cards.append(
            _CARD.substitute(
                rid=_esc(r.get("id") or url),
                url=_esc(url),
                url_attr=_esc(url, True),
                domain=_esc(domain),
                fav="https://icons.duckduckgo.com/ip3/" + _esc(domain) + ".ico" if domain else "",
                title=_esc(title),
                title_attr=_esc(title, True),
                rid_attr=_esc(r.get("id") or url, True),
                qh=_esc(qh, True),
                snippet_html=f"<p class='snippet'>{_esc(snippet)}</p>" if snippet else "",
                preview_html=(
                    f"<details class='preview'><summary>cached page text preview</summary>"
                    f"<div class='preview-text'>{preview}</div></details>"
                    if preview
                    else ""
                ),
            )
        )
    hits = len(results)
    bits = [f"{hits} result{'s' if hits != 1 else ''}"]
    source = payload.get("_source")
    if source:
        bits.append(f"from {_esc(source)}")
    if payload.get("_age"):
        bits.append(f"{_esc(payload['_age'])} old")
    if payload.get("_ttl_left"):
        bits.append(f"ttl {_esc(payload['_ttl_left'])} left")
    meta = " - ".join(bits)
    return "\n".join(cards), meta


def _fmt_dur(s: int) -> str:
    if s < 0:
        return "0s"
    if s < 60:
        return f"{s}s"
    if s < 3600:
        return f"{s // 60}m"
    if s < 86400:
        return f"{s // 3600}h {s % 3600 // 60}m"
    return f"{s // 86400}d {s % 86400 // 3600}h"


def render_search(
    initial_query: str = "",
    initial_results: dict | None = None,
    share: dict | None = None,
    page: int = 1,
) -> tuple[str, str]:
    results_html = ""
    meta_html = ""
    if initial_results is not None:
        results_html, meta_html = _render_cards(initial_results)
    if share:
        bits = []
        if share.get("result_count") is not None:
            n = share["result_count"]
            bits.append(f"{n} result{'s' if n != 1 else ''}")
        if share.get("source"):
            bits.append(f"from {_esc(share['source'])}")
        if share.get("age_s"):
            bits.append(f"{_fmt_dur(share['age_s'])} old")
        if share.get("ttl_left_s") is not None:
            bits.append(f"ttl {_fmt_dur(share['ttl_left_s'])} left")
        meta_html = " - ".join(bits)
    body = _SEARCH.substitute(
        q_esc=_esc(initial_query),
        results_html=results_html,
        meta_html=meta_html,
        share_html="",
        meta_hidden="" if initial_results is not None else "hidden",
        landing="yes" if not initial_query else "no",
        autofocus="autofocus" if not initial_query else "",
        pager_html=_search_pager(initial_query, initial_results, page),
    )
    return "Search", body


def _search_pager(q: str, payload: dict | None, page: int) -> str:
    if payload is None:
        return ""
    n = len(payload.get("results") or [])
    total = payload.get("_total_pages")
    if not total:
        # server returns one page per request; a full page hints at more
        if n < 10:
            return ""
        total = page + 1
    qp = html.escape(q)
    parts = []
    if page > 1:
        parts.append(f"<a href='/search?q={qp}&amp;p={page - 1}' rel='prev'>previous</a>")
    parts.append(f"<span class='pg-label'>page {page} of {total}</span>")
    if page < total:
        parts.append(f"<a href='/search?q={qp}&amp;p={page + 1}' rel='next'>next &gt;</a>")
    return " ".join(parts)


def render_history(
    cache: TTLCache,
    q: str | None,
    since_hours: int | None,
) -> tuple[str, str]:
    cs = cache.click_stats()
    stats_line = (
        f"{cs['last_24h']} clicks in last 24h · {cs['total']} total · "
        f"{'—' if not cs['oldest'] else _fmt_ts(cs['oldest']) + ' (oldest)'}"
    )
    rows = cache.get_clicks(query_text=q, limit=200, since_hours=since_hours)
    if not rows:
        rows_html = '<tr><td colspan="5" class="empty">no clicks yet — open a result from the <a href="/">search</a> page.</td></tr>'
    else:
        rendered = []
        for r in rows:
            rendered.append(
                "<tr>"
                f"<td class='ts'>{_esc(_fmt_ts(r['clicked_at']))}</td>"
                f"<td><a href='/row/{_esc(r['query_hash'])}'>{_esc(r['query'] or '(no query)')}</a></td>"
                f"<td>{_esc(r['title'])}</td>"
                f"<td class='url'><a href='{_esc(r['url'])}' rel='noopener noreferrer' target='_blank'>{_esc(r['url'][:80])}{'…' if len(r['url']) > 80 else ''}</a></td>"
                f"<td class='src'>{_esc(r['source'])}</td>"
                "</tr>"
            )
        rows_html = "\n".join(rendered)

    clear_link = "<a href='/history' class='clear'>clear</a>" if (q or since_hours is not None) else ""

    body = _HISTORY.substitute(
        stats_line=stats_line,
        q_esc=_esc(q or ""),
        sel_24="selected" if since_hours == 24 else "",
        sel_168="selected" if since_hours == 168 else "",
        sel_720="selected" if since_hours == 720 else "",
        sel_all="selected" if since_hours is None else "",
        rows_html=rows_html,
        clear_link=clear_link,
    )
    return "History", body


def _qs(q: str | None, include_expired: bool, page: int) -> str:
    parts = []
    if q:
        parts.append(f"q={html.escape(q)}")
    if include_expired:
        parts.append("include_expired=1")
    if page:
        parts.append(f"page={page}")
    return "&".join(parts)


def _render_row(cache: TTLCache, key: str) -> str | None:
    payload = cache.peek(key)
    if payload is None:
        return None
    now = int(time.time())
    rows = cache.list_rows(include_expired=True, limit=10000)
    meta_row = next((r for r in rows if r["hash"] == key), None)
    n_results = len(payload.get("results") or [])
    raw = json.dumps(payload, indent=2, ensure_ascii=False)
    rendered_results = []
    for i, res in enumerate(payload.get("results") or [], 1):
        title = _esc(res.get("title") or "(untitled)")
        url = _esc(res.get("url") or "")
        author = _esc(res.get("author") or "")
        published = _esc(res.get("publishedDate") or "")
        text = _esc(_first_preview(res.get("text"), 320))
        highlights = res.get("highlights") or []
        highlight_html = ""
        if highlights:
            hl_items = "".join(f"<li>{_esc(h)}</li>" for h in highlights[:5])
            highlight_html = f"<details class='hl'><summary>highlights ({len(highlights)})</summary><ul>{hl_items}</ul></details>"
        meta_bits = []
        if author:
            meta_bits.append(author)
        if published:
            meta_bits.append(published)
        meta = " · ".join(meta_bits)
        rendered_results.append(
            f"<article class='result'>"
            f"<h3><span class='idx'>{i}.</span> <a href='{url}' rel='noopener noreferrer' target='_blank'>{title}</a></h3>"
            f"<div class='url'><a href='{url}' rel='noopener noreferrer' target='_blank'>{url}</a></div>"
            + (f"<div class='meta'>{meta}</div>" if meta else "")
            + (f"<p class='text'>{text}</p>" if text else "")
            + highlight_html
            + "</article>"
        )
    results_html = "\n".join(rendered_results) if rendered_results else "<p class='empty'>no results in this cached response.</p>"

    hits = meta_row["hits"] if meta_row else "?"
    size = _fmt_bytes(meta_row["size_bytes"]) if meta_row else "—"
    expires_at = meta_row["expires_at"] if meta_row else 0
    body = _ROW.substitute(
        q_esc=_esc(payload.get("_q", "(no query)")),
        hash=_esc(key),
        hits=str(hits),
        n_results=n_results,
        cost=(payload.get("costDollars") or {}).get("total", 0) if isinstance(payload.get("costDollars"), dict) else 0,
        size=size,
        expires_in=_esc(_fmt_remaining(expires_at, now)) if expires_at else "—",
        expires_at=_esc(_fmt_ts(expires_at)) if expires_at else "—",
        search_type=_esc(payload.get("searchType", "")),
        request_id=_esc(payload.get("requestId", "")),
        results_html=results_html,
        raw=_esc(raw),
    )
    return body


def render_shell(title: str, body: str, version: str, page_class: str = "") -> str:
    def _active(suffix: str) -> str:
        return " class='active'" if page_class == suffix else ""

    return _SHELL.substitute(
        title=_esc(title),
        body=body,
        css=_BASE_CSS,
        js=_BASE_JS,
        ver=_esc(version),
        page_class=_esc(page_class),
        nav_search=_active("search"),
        nav_history=_active("history"),
        nav_cache=_active("cache"),
    )
