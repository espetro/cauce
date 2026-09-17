"""Build the stats dashboard from the search_log table.

Two producers share the same aggregation: ``build()`` writes a
self-contained ``index.html`` with inline SVG charts (no JS), ``build_json()``
returns the same aggregates as a typed ``StatsSummary`` for an eventual JSON
API. Ported from legacy ``oxe/stats.py``, tightened to the data ladder:
``build_json`` used to return a bare ``dict``, and DB row tuples are now
``NamedTuple``s instead of positional tuples threaded through by hand.
"""

import contextlib
import html
import sqlite3
import time
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import NamedTuple, cast

from pydantic import BaseModel, ConfigDict

from . import sqlload

PANEL_COUNT = 6
_MIN_POLYLINE_POINTS = 2


class DailyRow(NamedTuple):
    ts: int
    source: str
    duration_ms: int | None


@dataclass(frozen=True)
class TopQueryRow:
    """``count`` collides with tuple.count(), so this is a dataclass, not a
    NamedTuple (basedpyright: reportIncompatibleMethodOverride)."""

    query: str
    count: int


class ZeroResultRow(NamedTuple):
    query: str
    last_seen: int


@dataclass(frozen=True)
class ClientSplitRow:
    """See ``TopQueryRow`` for why this is a dataclass rather than a NamedTuple."""

    client: str
    count: int


class SearchesPerDay(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    day: str
    cache: int
    network: int
    total: int


class HitRate(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    total: int
    cache_hits: int
    rate: float | None


class LatencyMs(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    p50: float | None
    p90: float | None
    p99: float | None


class TopQuery(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    query: str
    count: int


class ZeroResultQuery(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    query: str
    last_seen: int


class ClientSplit(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    client: str
    count: int


class StatsSummary(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    days: int
    searches_per_day: list[SearchesPerDay]
    hit_rate: HitRate
    latency_ms: LatencyMs
    top_queries: list[TopQuery]
    zero_result_queries: list[ZeroResultQuery]
    client_split: list[ClientSplit]


def _connect(db_path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.execute("PRAGMA query_only=1")
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


def _run(fn: sqlload.QueryFn, conn: sqlite3.Connection, **kwargs: object) -> object:
    try:
        return fn(conn, **kwargs)
    except sqlite3.OperationalError as exc:
        if "database is locked" not in str(exc):
            raise
    time.sleep(0.5)  # single retry after a short wait
    return fn(conn, **kwargs)


def _has_search_log(conn: sqlite3.Connection) -> bool:
    return bool(_run(sqlload.query("has_search_log"), conn))


def _since_clause(days: int) -> int:
    return int((datetime.now(timezone.utc) - timedelta(days=days)).timestamp())


def _day_keys(days: int) -> list[str]:
    today = datetime.now(timezone.utc).date()
    return [(today - timedelta(days=days - 1 - i)).isoformat() for i in range(days)]


def _ts_day(ts: int) -> str:
    return datetime.fromtimestamp(ts, tz=timezone.utc).date().isoformat()


def _fetch_daily(conn: sqlite3.Connection, days: int) -> list[DailyRow]:
    raw = cast(
        list[tuple[object, object, object]],
        _run(sqlload.query("stat_daily"), conn, cutoff=_since_clause(days)),
    )
    return [
        DailyRow(ts=cast(int, r[0]), source=cast(str, r[1]), duration_ms=cast(int | None, r[2]))
        for r in raw
    ]


def _fetch_top_queries(conn: sqlite3.Connection, days: int, limit: int = 20) -> list[TopQueryRow]:
    raw = cast(
        list[tuple[object, object]],
        _run(sqlload.query("stat_top_queries"), conn, cutoff=_since_clause(days), limit=limit),
    )
    return [TopQueryRow(query=cast(str, r[0]), count=cast(int, r[1])) for r in raw]


def _fetch_zero_result(conn: sqlite3.Connection, days: int, limit: int = 50) -> list[ZeroResultRow]:
    raw = cast(
        list[tuple[object, object]],
        _run(sqlload.query("stat_zero_result"), conn, cutoff=_since_clause(days), limit=limit),
    )
    return [ZeroResultRow(query=cast(str, r[0]), last_seen=cast(int, r[1])) for r in raw]


def _fetch_client_split(conn: sqlite3.Connection, days: int) -> list[ClientSplitRow]:
    raw = cast(
        list[tuple[object, object]],
        _run(sqlload.query("stat_client_split"), conn, cutoff=_since_clause(days)),
    )
    return [ClientSplitRow(client=cast(str, r[0]), count=cast(int, r[1])) for r in raw]


def _percentile(sorted_vals: list[int], p: float) -> float:
    """Percentile of a non-empty, pre-sorted list. Callers guard emptiness."""
    k = (len(sorted_vals) - 1) * p / 100
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    return sorted_vals[f] + (sorted_vals[c] - sorted_vals[f]) * (k - f)


def panel_searches_per_day(rows: list[DailyRow], days: int) -> str:
    by_day = {d: {"cache": 0, "network": 0} for d in _day_keys(days)}
    for ts, source, _duration in rows:
        d = _ts_day(ts)
        if d in by_day:
            by_day[d][source if source in ("cache", "network") else "network"] += 1
    if not any(v["cache"] + v["network"] for v in by_day.values()):
        return '<p class="empty">no data yet</p>'
    totals = [by_day[d]["cache"] + by_day[d]["network"] for d in by_day]
    ymax = max(totals) or 1
    w, h = 520, 220
    pad_l, pad_b, pad_t = 34, 22, 8
    plot_w, plot_h = w - pad_l - 8, h - pad_b - pad_t
    n = len(by_day)
    bw = plot_w / n
    parts = [f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="searches per day">']
    for i in range(5, -1, -2):
        y = pad_t + plot_h * (1 - i / ymax)
        val = round(ymax * i / 5)
        parts.append(
            f'<line x1="{pad_l}" y1="{y:.1f}" x2="{w - 8}" y2="{y:.1f}" class="grid"/>'
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
    day_keys = _day_keys(days)
    parts.append(f'<text x="{pad_l}" y="{h - 6}" class="axis">{day_keys[0][5:]}</text>')
    parts.append(
        f'<text x="{w - 8}" y="{h - 6}" class="axis" text-anchor="end">{day_keys[-1][5:]}</text>'
    )
    parts.append(
        f'<rect x="{pad_l}" y="{pad_t - 2}" width="10" height="10" class="bar-cache"/>'
        f'<text x="{pad_l + 14}" y="{pad_t + 7}" class="axis">cache</text>'
        f'<rect x="{pad_l + 60}" y="{pad_t - 2}" width="10" height="10" class="bar-net"/>'
        f'<text x="{pad_l + 74}" y="{pad_t + 7}" class="axis">network</text>'
    )
    parts.append("</svg>")
    return "".join(parts)


def panel_hit_rate(rows: list[DailyRow], days: int) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    hits = sum(1 for _ts, s, _duration in rows if s == "cache")
    rate = round(100 * hits / len(rows))
    by_day: dict[str, list[int]] = {d: [0, 0] for d in _day_keys(days)}
    for ts, s, _duration in rows:
        d = _ts_day(ts)
        if d in by_day:
            by_day[d][0 if s == "cache" else 1] += 1
    series = [100 * v[0] / (v[0] + v[1]) if v[0] + v[1] else None for v in by_day.values()]
    w, h = 520, 90
    pts = [
        (i * w / (len(series) - 1 or 1), h - 6 - (h - 16) * v / 100)
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
        f'{big}<svg viewBox="0 0 {w} {h}" role="img" aria-label="cache hit rate trend">'
        f"{spark}</svg>"
    )


def panel_latency(rows: list[DailyRow], days: int) -> str:
    by_day: dict[str, list[int]] = {d: [] for d in _day_keys(days)}
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
    w, h = 520, 200
    pad = 10
    n = len(p50s)

    def polyline(series: list[float | None]) -> str:
        pts = [
            (pad + i * (w - 2 * pad) / (n - 1 or 1), h - pad - (h - 2 * pad) * v / ymax)
            for i, v in enumerate(series)
            if v is not None
        ]
        if len(pts) < _MIN_POLYLINE_POINTS:
            return ""
        return (
            f'<polyline points="{" ".join(f"{x:.1f},{y:.1f}" for x, y in pts)}" '
            f'fill="none" class="line-p50"/>'
        )

    lines = polyline(p50s) + polyline(p95s).replace("line-p50", "line-p95")
    grid = (
        f'<text x="{pad}" y="{h - pad + 4}" class="axis">0 ms</text>'
        f'<text x="{pad}" y="{pad + 8}" class="axis">{ymax:.0f} ms</text>'
    )
    legend = (
        f'<line x1="{pad}" y1="{pad}" x2="{pad + 24}" y2="{pad}" class="line-p50"/>'
        f'<text x="{pad + 30}" y="{pad + 4}" class="axis">p50</text>'
        f'<line x1="{pad + 80}" y1="{pad}" x2="{pad + 104}" y2="{pad}" class="line-p95"/>'
        f'<text x="{pad + 110}" y="{pad + 4}" class="axis">p95</text>'
    )
    return (
        f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="network latency">'
        f"{grid}{lines}{legend}</svg>"
    )


def panel_top_queries(rows: list[TopQueryRow]) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    out = ['<table class="ptable"><thead><tr><th>#</th><th>query</th><th>count</th></tr></thead>']
    out.append("<tbody>")
    for i, row in enumerate(rows, 1):
        label = html.escape(row.query or "(empty)")
        out.append(f"<tr><td>{i}</td><td>{label}</td><td>{row.count}</td></tr>")
    out.append("</tbody></table>")
    return "".join(out)


def panel_zero_result(rows: list[ZeroResultRow]) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    out = ['<table class="ptable"><thead><tr><th>query</th><th>last seen</th></tr></thead><tbody>']
    for text, ts in rows:
        seen = datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%d %H:%M")
        out.append(f"<tr><td>{html.escape(text or '(empty)')}</td><td>{seen}</td></tr>")
    out.append("</tbody></table>")
    return "".join(out)


def panel_client_split_rows(rows: list[ClientSplitRow]) -> str:
    if not rows:
        return '<p class="empty">no data yet</p>'
    counts = {row.client: row.count for row in rows}
    total = sum(counts.values())
    parts: list[str] = []
    for client in ["mcp", "http", "web-ui"]:
        if client not in counts:
            continue
        pct = 100 * counts[client] / total
        parts.append(
            f'<div class="hbar-row"><span class="hbar-label">{client}</span>'
            f'<span class="hbar-track"><span class="hbar-fill" style="width:{pct:.0f}%">'
            f"</span></span>"
            f'<span class="hbar-pct">{pct:.0f}%</span></div>'
        )
    return "".join(parts)


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
.ptable td:last-child, .ptable th:last-child { text-align: right; }
.hbar-row { display: flex; align-items: center; gap: 0.6rem; margin: 0.5rem 0; }
.hbar-label { width: 3.5rem; font-size: 0.85rem; }
.hbar-track { flex: 1; height: 0.9rem; background: var(--grid); border-radius: 4px; }
.hbar-fill { display: block; height: 100%; background: var(--accent); border-radius: 4px; }
.hbar-pct { width: 3rem; text-align: right; font-size: 0.85rem; color: var(--muted); }
@media (max-width: 700px) { .grid { grid-template-columns: 1fr; } }
"""


def _panel(title: str, body: str, *, wide: bool = False) -> str:
    cls = "panel wide" if wide else "panel"
    return f'<section class="{cls}"><h2>{title}</h2>{body}</section>'


def _render_page(panels: list[str], days: int, generated: str) -> str:
    return (
        "<!doctype html>\n"
        '<html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width, initial-scale=1">'
        "<title>oxe stats</title>"
        f"<style>{CSS}</style></head><body>\n"
        "<header><h1>oxe stats</h1>"
        f'<p class="meta">generated {generated} &middot; window: last {days} days</p></header>\n'
        f"{''.join(panels)}\n</body></html>\n"
    )


def _empty_panels() -> list[str]:
    empty = '<p class="empty">no data yet</p>'
    return [
        _panel("searches per day", empty),
        _panel("cache hit rate", empty),
        _panel("network latency", empty),
        _panel("client split", empty),
        _panel("top queries", empty, wide=True),
        _panel("zero-result queries", empty, wide=True),
    ]


def _panels_for(conn: sqlite3.Connection, days: int) -> list[str]:
    if not _has_search_log(conn):
        return _empty_panels()
    rows = _fetch_daily(conn, days)
    top = _fetch_top_queries(conn, days)
    zeros = _fetch_zero_result(conn, days)
    clients = _fetch_client_split(conn, days)
    return [
        _panel("searches per day", panel_searches_per_day(rows, days)),
        _panel("cache hit rate", panel_hit_rate(rows, days)),
        _panel("network latency", panel_latency(rows, days)),
        _panel("client split", panel_client_split_rows(clients)),
        _panel("top queries", panel_top_queries(top), wide=True),
        _panel("zero-result queries", panel_zero_result(zeros), wide=True),
    ]


def build(db_path: str, out_dir: str, days: int = 30) -> str:
    """Build the dashboard and return the path of the written index.html."""
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M")
    try:
        with contextlib.closing(_connect(db_path)) as conn:
            panels = _panels_for(conn, days)
    except sqlite3.OperationalError:
        panels = _empty_panels()

    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    path = out / "index.html"
    path.write_text(_render_page(panels, days, generated), encoding="utf-8")
    return str(path)


def _empty_aggregates() -> tuple[
    list[DailyRow], list[TopQueryRow], list[ZeroResultRow], list[ClientSplitRow]
]:
    return [], [], [], []


def _fetch_all(
    conn: sqlite3.Connection, days: int
) -> tuple[list[DailyRow], list[TopQueryRow], list[ZeroResultRow], list[ClientSplitRow]]:
    if not _has_search_log(conn):
        return _empty_aggregates()
    return (
        _fetch_daily(conn, days),
        _fetch_top_queries(conn, days),
        _fetch_zero_result(conn, days),
        _fetch_client_split(conn, days),
    )


def build_json(db_path: str, days: int = 14) -> StatsSummary:
    """Return dashboard aggregates as a typed summary (sister of build())."""
    try:
        with contextlib.closing(
            sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=5)
        ) as conn:
            daily_rows, top_queries, zero_result, client_split = _fetch_all(conn, days)
    except sqlite3.OperationalError:
        daily_rows, top_queries, zero_result, client_split = _empty_aggregates()

    per_day: dict[str, dict[str, int]] = {d: {"cache": 0, "network": 0} for d in _day_keys(days)}
    for row in daily_rows:
        d = _ts_day(row.ts)
        if d in per_day:
            per_day[d]["cache" if row.source == "cache" else "network"] += 1
    total = len(daily_rows)
    hits = sum(1 for row in daily_rows if row.source == "cache")
    lats = sorted(row.duration_ms for row in daily_rows if row.duration_ms is not None)

    return StatsSummary(
        days=days,
        searches_per_day=[
            SearchesPerDay(
                day=d, cache=v["cache"], network=v["network"], total=v["cache"] + v["network"]
            )
            for d, v in per_day.items()
        ],
        hit_rate=HitRate(
            total=total,
            cache_hits=hits,
            rate=round(100 * hits / total, 1) if total else None,
        ),
        latency_ms=LatencyMs(
            p50=round(_percentile(lats, 50), 1) if lats else None,
            p90=round(_percentile(lats, 90), 1) if lats else None,
            p99=round(_percentile(lats, 99), 1) if lats else None,
        ),
        top_queries=[TopQuery(query=t.query, count=t.count) for t in top_queries],
        zero_result_queries=[
            ZeroResultQuery(query=z.query, last_seen=z.last_seen) for z in zero_result
        ],
        client_split=[ClientSplit(client=c.client, count=c.count) for c in client_split],
    )
