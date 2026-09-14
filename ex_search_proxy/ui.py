import html
import json
import time
from string import Template
from typing import Any

from .cache import TTLCache

PAGE = 50
ASSET_VERSION = "1"


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


def _esc(s: Any) -> str:
    if s is None:
        return ""
    return html.escape(str(s))


def _first_preview(text: str | None, n: int = 280) -> str:
    if not text:
        return ""
    text = " ".join(text.split())
    if len(text) <= n:
        return text
    return text[: n - 1] + "…"


_BASE_CSS = "/static/ui.css?v=" + ASSET_VERSION
_BASE_JS = "/static/ui.js?v=" + ASSET_VERSION


_SHELL = Template("""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title} · ex-search-proxy</title>
<link rel="stylesheet" href="${css}">
</head>
<body>
<header class="top">
  <a class="brand" href="/">ex-search-proxy</a>
  <nav>
    <a href="/">cache</a>
    <a href="/cache/stats">stats</a>
    <a href="/health">health</a>
    <a href="/docs">api</a>
  </nav>
  <span class="ver">v${ver}</span>
</header>
<main>
${body}
</main>
<script src="${js}" defer></script>
</body>
</html>
""")


_INDEX = Template("""<section class="hero">
  <h1>Cached searches</h1>
  <p>${stats_line}</p>
</section>
<form class="filters" method="get" action="/">
  <input type="search" name="q" placeholder="filter by query text…" value="${q_esc}">
  <label class="check"><input type="checkbox" name="include_expired" ${exp_check}> include expired</label>
  <button type="submit">filter</button>
  ${clear_link}
</form>
<table class="rows">
  <thead>
    <tr>
      <th>query</th>
      <th>hits</th>
      <th>results</th>
      <th>size</th>
      <th>expires in</th>
      <th>expires at</th>
      <th>hash</th>
    </tr>
  </thead>
  <tbody>
${rows_html}
  </tbody>
</table>
<div class="pager">
  ${pager_html}
</div>
""")


_ROW = Template("""<section class="row-head">
  <a href="/" class="back">← back to cache</a>
  <h1>${q_esc}</h1>
  <dl class="meta">
    <dt>hash</dt><dd class="hash">${hash}</dd>
    <dt>hits</dt><dd>${hits}</dd>
    <dt>results</dt><dd>${n_results}</dd>
    <dt>cost</dt><dd>${cost}</dd>
    <dt>size</dt><dd>${size}</dd>
    <dt>expires</dt><dd>${expires_in} (${expires_at})</dd>
    <dt>search type</dt><dd>${search_type}</dd>
    <dt>request id</dt><dd class="hash">${request_id}</dd>
  </dl>
  <form method="post" action="/row/${hash}/delete" class="danger" data-confirm="Delete this cached search?">
    <button type="submit">delete from cache</button>
  </form>
</section>
<section class="results">
${results_html}
</section>
<section class="raw">
  <details>
    <summary>raw payload</summary>
    <pre>${raw}</pre>
  </details>
</section>
""")


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
        pager_bits.append(f"<a href='/?{qp}'>← prev</a>")
    if len(rows) == PAGE:
        qp = _qs(q, include_expired, page + 1)
        pager_bits.append(f"<a href='/?{qp}'>next →</a>")
    pager_html = " ".join(pager_bits) if pager_bits else f"<span class='muted'>page {page + 1} · {total_rows} total rows</span>"

    clear_link = "<a href='/' class='clear'>clear</a>" if (q or include_expired) else ""
    body = _INDEX.substitute(
        stats_line=stats_line,
        q_esc=_esc(q or ""),
        exp_check="checked" if include_expired else "",
        rows_html=rows_html,
        pager_html=pager_html,
        clear_link=clear_link,
    )
    return "Cache", body


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


def render_shell(title: str, body: str, version: str) -> str:
    return _SHELL.substitute(
        title=_esc(title),
        body=body,
        css=_BASE_CSS,
        js=_BASE_JS,
        ver=_esc(version),
    )
