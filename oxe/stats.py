"""Build the static stats dashboard from the search_log table.

Usage: oxe stats [--db PATH] [--out DIR] [--days N]
Writes a single self-contained index.html with inline SVG charts, no JS.
"""

import html
import sqlite3
from datetime import datetime, timedelta, timezone
from pathlib import Path

from . import sqlload

PANEL_COUNT = 6


def _connect(db_path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.execute("PRAGMA query_only=1")
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


def _run(fn, conn, **kwargs):
    try:
        return fn(conn, **kwargs)
    except sqlite3.OperationalError as exc:
        if "database is locked" not in str(exc):
            raise
    # single retry after a short wait
    import time

    time.sleep(0.5)
    return fn(conn, **kwargs)


def _has_search_log(conn: sqlite3.Connection) -> bool:
    return bool(_run(sqlload.queries().has_search_log, conn))


def _since_clause(days: int):
    cutoff = int((datetime.now(timezone.utc) - timedelta(days=days)).timestamp())
    return cutoff


def _day_keys(days: int) -> list[str]:
    today = datetime.now(timezone.utc).date()
    return [(today - timedelta(days=days - 1 - i)).isoformat() for i in range(days)]


def _ts_day(ts: int) -> str:
    return datetime.fromtimestamp(ts, tz=timezone.utc).date().isoformat()


def _fetch_daily(conn, days):
    return list(_run(sqlload.queries().stat_daily, conn, cutoff=_since_clause(days)))


def panel_searches_per_day(rows, days) -> str:
    by_day = {d: {"cache": 0, "network": 0} for d in _day_keys(days)}
    for ts, source, _ in rows:
        d = _ts_day(ts)
        if d in by_day:
            by_day[d][source if source in ("cache", "network") else "network"] += 1
    if not any(v["cache"] + v["network"] for v in by_day.values()):
        return '<p class="empty">no data yet</p>'
    totals = [by_day[d]["cache"] + by_day[d]["network"] for d in by_day]
    ymax = max(totals) or 1
    W, H = 520, 220
    pad_l, pad_b, pad_t = 34, 22, 8
    plot_w, plot_h = W - pad_l - 8, H - pad_b - pad_t
    n = len(by_day)
    bw = plot_w / n
    parts = [f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="searches per day">']
    for i in range(5, -1, -2):
        y = pad_t + plot_h * (1 - i / ymax)
        val = round(ymax * i / 5)
        parts.append(
            f'<line x1="{pad_l}" y1="{y:.1f}" x2="{W - 8}" y2="{y:.1f}" class="grid"/>'
            f'<text x="{pad_l - 5}" y="{y + 4:.1f}" class="axis" text-anchor="end">{val}</text>'
        )
    for i, d in enumerate(by_day):
        x = pad_l + i * bw
        hc = plot_h * by_day[d]["cache"] / ymax
        hn = plot_h * by_day[d]["network"] / ymax
        if by_day[d]["cache"]:
            parts.append(
                f'<rect x="{x + bw * 0.15:.1f}" y="{pad_t + plot_h - hc:.1f}" '
                f'width="{bw * 0.7:.1f}" height="{hc:.1f}" class="bar-cache"/>'
            )
        if by_day[d]["network"]:
            parts.append(
                f'<rect x="{x + bw * 0.15:.1f}" y="{pad_t + plot_h - hc - hn:.1f}" '
                f'width="{bw * 0.7:.1f}" height="{hn:.1f}" class="bar-net"/>'
            )
    parts.append(
        f'<text x="{pad_l}" y="{H - 6}" class="axis">{_day_keys(days)[0][5:]}</text>'
    )
    parts.append(
        f'<text x="{W - 8}" y="{H - 6}" class="axis"'
        f' text-anchor="end">{_day_keys(days)[-1][5:]}</text>'
    )
    parts.append(
        f'<rect x="{pad_l}" y="{pad_t - 2}" width="10" height="10" class="bar-cache"/>'
        f'<text x="{pad_l + 14}" y="{pad_t + 7}" class="axis">cache</text>'
        f'<rect x="{pad_l + 60}" y="{pad_t - 2}" width="10" height="10" class="bar-net"/>'
        f'<text x="{pad_l + 74}" y="{pad_t + 7}" class="axis">network</text>'
    )
    parts.append("</svg>")
    return "".join(parts)


def panel_hit_rate(rows, days) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    hits = sum(1 for _, s, _ in rows if s == "cache")
    rate = round(100 * hits / len(rows))
    by_day = {d: [0, 0] for d in _day_keys(days)}
    for ts, s, _ in rows:
        d = _ts_day(ts)
        if d in by_day:
            by_day[d][0 if s == "cache" else 1] += 1
    series = [100 * v[0] / (v[0] + v[1]) if v[0] + v[1] else None for v in by_day.values()]
    W, H = 520, 90
    pts = [
        (i * W / (len(series) - 1 or 1), H - 6 - (H - 16) * v / 100)
        for i, v in enumerate(series)
        if v is not None
    ]
    spark = ""
    if len(pts) > 1:
        spark = (
            f'<polyline points="{" ".join(f"{x:.1f},{y:.1f}" for x, y in pts)}" '
            f'class="line-p50" fill="none"/>'
        )
    big = f'<div class="bignum">{rate}%</div>'
    return (
        f'{big}<svg viewBox="0 0 {W} {H}" role="img" aria-label="cache hit rate trend">'
        f'{spark}</svg>'
    )


def _percentile(sorted_vals, p):
    if not sorted_vals:
        return None
    k = (len(sorted_vals) - 1) * p / 100
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    return sorted_vals[f] + (sorted_vals[c] - sorted_vals[f]) * (k - f)


def panel_latency(rows, days) -> str:
    by_day = {d: [] for d in _day_keys(days)}
    for ts, source, dur in rows:
        if source == "network" and dur is not None:
            d = _ts_day(ts)
            if d in by_day:
                by_day[d].append(dur)
    if not any(by_day.values()):
        return '<p class="empty">no data yet</p>'
    p50s = [_percentile(sorted(v), 50) if v else None for v in by_day.values()]
    p95s = [_percentile(sorted(v), 95) if v else None for v in by_day.values()]
    known = [v for v in p50s + p95s if v is not None]
    ymax = max(known) or 1
    W, H = 520, 200
    pad = 10
    n = len(p50s)

    def polyline(series):
        pts = [
            (pad + i * (W - 2 * pad) / (n - 1 or 1), H - pad - (H - 2 * pad) * v / ymax)
            for i, v in enumerate(series)
            if v is not None
        ]
        if len(pts) < 2:
            return ""
        return (
            f'<polyline points="{" ".join(f"{x:.1f},{y:.1f}" for x, y in pts)}" '
            f'fill="none" class="line-p50"/>'
        )

    lines = polyline(p50s) + polyline(p95s).replace("line-p50", "line-p95")
    grid = (
        f'<text x="{pad}" y="{H - pad + 4}" class="axis">0 ms</text>'
        f'<text x="{pad}" y="{pad + 8}" class="axis">{ymax:.0f} ms</text>'
    )
    legend = (
        f'<line x1="{pad}" y1="{pad}" x2="{pad + 24}" y2="{pad}" class="line-p50"/>'
        f'<text x="{pad + 30}" y="{pad + 4}" class="axis">p50</text>'
        f'<line x1="{pad + 80}" y1="{pad}" x2="{pad + 104}" y2="{pad}" class="line-p95"/>'
        f'<text x="{pad + 110}" y="{pad + 4}" class="axis">p95</text>'
    )
    return (
        f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="network latency">'
        f"{grid}{lines}{legend}</svg>"
    )




def _fetch_top_queries(conn, days, limit=20):
    return list(
        _run(sqlload.queries().stat_top_queries, conn, cutoff=_since_clause(days), limit=limit)
    )


def panel_top_queries(rows) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    out = [
        '<table class="ptable"><thead>'
        '<tr><th>#</th><th>query</th><th>count</th></tr></thead><tbody>'
    ]
    for i, (text, count) in enumerate(rows, 1):
        out.append(
            f"<tr><td>{i}</td><td>{html.escape(text or '(empty)')}</td><td>{count}</td></tr>"
        )
    out.append("</tbody></table>")
    return "".join(out)


def _fetch_zero_result(conn, days, limit=50):
    return list(
        _run(sqlload.queries().stat_zero_result, conn, cutoff=_since_clause(days), limit=limit)
    )


def panel_zero_result(rows) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    out = ['<table class="ptable"><thead><tr><th>query</th><th>last seen</th></tr></thead><tbody>']
    for text, ts in rows:
        seen = datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%d %H:%M")
        out.append(
            f'<tr><td>{html.escape(text or "(empty)")}</td><td>{seen}</td></tr>'
        )
    out.append("</tbody></table>")
    return "".join(out)


CSS = """\
:root { color-scheme: light dark; --fg: #111; --muted: #666; --bg: #fff; \
--grid: #e5e5e5; --accent: #2563eb; --accent2: #f59e0b; --bar2: #93c5fd; }
@media (prefers-color-scheme: dark) { :root { --fg: #eee; --muted: #999; \
--bg: #111; --grid: #333; --bar2: #1e3a8a; } }
* { box-sizing: border-box; }
body { font-family: geist, ui-sans-serif, system-ui, sans-serif; color: var(--fg); \
background: var(--bg); margin: 0 auto; padding: 1.5rem; max-width: 60rem; }
header h1 { margin: 0; font-size: 1.4rem; }
header .meta { color: var(--muted); font-size: 0.85rem; margin: 0.25rem 0 1.25rem; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr)); gap: 1rem; }
.panel { border: 1px solid var(--grid); border-radius: 8px; padding: 1rem; min-width: 0; }
.panel.wide { grid-column: 1 / -1; }
.panel h2 { margin: 0 0 0.75rem; font-size: 0.95rem; }
.panel svg { width: 100%; height: auto; display: block; }
.grid line.grid, .gridline { stroke: var(--grid); }
svg line.grid { stroke: var(--grid); }
.axis { fill: var(--muted); font-size: 10px; }
.bar-cache { fill: var(--accent); }
.bar-net { fill: var(--bar2); }
.line-p50 { stroke: var(--accent); stroke-width: 2; }
.line-p95 { stroke: var(--accent2); stroke-width: 2; stroke-dasharray: 5 4; }
.bignum { font-size: 3rem; font-weight: 700; line-height: 1.1; }
.empty { color: var(--muted); font-style: italic; }
.ptable { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
.ptable th { text-align: left; color: var(--muted); font-weight: 500; }
.ptable th, .ptable td { padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--grid); }
/* ptable right-align handled by td:last-child */
.ptable td:last-child, .ptable th:last-child { text-align: right; }
.hbar-row { display: flex; align-items: center; gap: 0.6rem; margin: 0.5rem 0; }
.hbar-label { width: 3.5rem; font-size: 0.85rem; }
.hbar-track { flex: 1; height: 0.9rem; background: var(--grid); border-radius: 4px; }
.hbar-fill { display: block; height: 100%; background: var(--accent); border-radius: 4px; }
.hbar-pct { width: 3rem; text-align: right; font-size: 0.85rem; color: var(--muted); }
@media (max-width: 700px) { .grid { grid-template-columns: 1fr; } }
"""


def _panel(title, body, wide=False):
    cls = "panel wide" if wide else "panel"
    return f'<section class="{cls}"><h2>{title}</h2>{body}</section>'


def _render_page(panels, days, generated) -> str:
    return (
        "<!doctype html>\n"
        '<html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width, initial-scale=1">'
        "<title>oxe stats</title>"
        f"<style>{CSS}</style></head><body>\n"
        "<header><h1>oxe stats</h1>"
        f'<p class="meta">generated {generated} &middot; window: last {days} days</p></header>\n'
        f'{"".join(panels)}\n</body></html>\n'
    )


def build(db_path: str, out_dir: str, days: int = 30) -> str:
    """Build the dashboard and return the path of the written index.html."""
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M")
    try:
        conn = _connect(db_path)
    except sqlite3.OperationalError:
        conn = None

    panels = []
    if conn is None or not _has_search_log(conn):
        empty = '<p class="empty">no data yet</p>'
        panels = [
            _panel("searches per day", empty),
            _panel("cache hit rate", empty),
            _panel("network latency", empty),
            _panel("client split", empty),
            _panel("top queries", empty, wide=True),
            _panel("zero-result queries", empty, wide=True),
        ]
    else:
        try:
            rows = _fetch_daily(conn, days)
            top = _fetch_top_queries(conn, days)
            zeros = _fetch_zero_result(conn, days)
            clients = _fetch_client_split(conn, days)
        finally:
            conn.close()
        panels = [
            _panel("searches per day", panel_searches_per_day(rows, days)),
            _panel("cache hit rate", panel_hit_rate(rows, days)),
            _panel("network latency", panel_latency(rows, days)),
            _panel("client split", panel_client_split_rows(clients)),
            _panel("top queries", panel_top_queries(top), wide=True),
            _panel("zero-result queries", panel_zero_result(zeros), wide=True),
        ]

    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    path = out / "index.html"
    path.write_text(_render_page(panels, days, generated), encoding="utf-8")
    return str(path)


def _fetch_client_split(conn, days):
    return list(_run(sqlload.queries().stat_client_split, conn, cutoff=_since_clause(days)))


def panel_client_split_rows(rows) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    counts = {client: n for client, n in rows}
    total = sum(counts.values())
    parts = []
    for client in ["mcp", "http", "web-ui"]:
        if client not in counts:
            continue
        pct = 100 * counts[client] / total
        parts.append(
            f'<div class="hbar-row"><span class="hbar-label">{client}</span>'
            f'<span class="hbar-track"><span class="hbar-fill" style="width:{pct:.0f}%">'
            f'</span></span>'
            f'<span class="hbar-pct">{pct:.0f}%</span></div>'
        )
    return "".join(parts)
