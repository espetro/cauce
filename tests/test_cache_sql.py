"""Tests for oxe.cache TTLCache backed by named SQL queries in oxe/sql/*.sql."""


from oxe.cache import TTLCache


def test_set_get_roundtrip(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    val = {"results": [1, 2], "_q": "hello"}
    c.set("h1", val, ttl=60)
    assert c.get("h1") == val
    c.close()


def test_ttl_expiry(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [1], "_q": "x"}, ttl=-1)
    assert c.get("h1") is None
    c.close()


def test_hits_increment(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "x"}, ttl=60)
    before = c.stats()["total_hits"]
    c.get("h1")
    assert c.stats()["total_hits"] >= before + 1
    c.close()


def test_list_rows_filters(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "python tips"}, ttl=60)
    c.set("h2", {"results": [], "_q": "golang tricks"}, ttl=-1)
    rows = c.list_rows()
    hashes = {r["hash"] for r in rows}
    assert "h1" in hashes and "h2" not in hashes
    all_rows = c.list_rows(include_expired=True)
    assert {"h1", "h2"} <= {r["hash"] for r in all_rows}
    sub = c.list_rows(q="python")
    assert [r["hash"] for r in sub] == ["h1"]
    paged = c.list_rows(include_expired=True, limit=1, offset=1)
    assert len(paged) == 1
    c.close()


def test_clicks(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "python tips"}, ttl=60)
    cid = c.record_click("h1", "r1", "https://x.com", "X")
    assert cid > 0
    assert len(c.get_clicks(query_hash="h1")) == 1
    assert c.get_clicks(query_hash="nope") == []
    assert len(c.get_clicks(query_text="python")) == 1
    assert c.get_clicks(query_text="nomatch") == []
    c.close()


def test_click_stats(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.record_click("h1", "r1", "https://x.com", "X")
    stats = c.click_stats()
    assert stats["total"] == 1
    assert stats["last_24h"] == 1
    c.close()


def test_search_log(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.log_search("python tips", "h1", source="mcp", result_count=3)
    c.log_search("golang", "h2", source="http", duration_ms=None)
    rows = c.get_search_log()
    assert len(rows) == 2
    by_hash = {r["query_hash"]: r for r in rows}
    r1 = by_hash["h1"]
    assert r1["query"] == "python tips"
    assert r1["source"] == "mcp"
    assert r1["result_count"] == 3
    assert by_hash["h2"]["duration_ms"] is None
    c.close()


def test_lookup_query_text(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "python tips"}, ttl=60)
    assert c.lookup_query_text("h1") == "python tips"
    c.log_search("golang docs", "h2", source="http")
    assert c.lookup_query_text("h2") == "golang docs"
    assert c.lookup_query_text("h3") is None
    c.close()


def test_prunes(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.record_click("h1", "r1", "https://x.com", "X")
    c.log_search("python", "h1", source="http")
    assert c.prune_clicks(retention_days=0) == 1
    assert c.prune_search_log(retention_days=0) == 1
    assert c.get_clicks() == []
    assert c.get_search_log() == []
    c.close()


def test_delete_clicks_scopes(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.record_click("h1", "r1", "https://x.com", "X")
    c.record_click("h2", "r2", "https://y.com", "Y")
    assert c.delete_clicks("bogus") == 0
    assert c.delete_clicks("24h") == 2
    c.record_click("h3", "r3", "https://z.com", "Z")
    assert c.delete_clicks("all") == 1
    assert c.get_clicks() == []
    c.close()


def test_invalidate(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "a"}, ttl=60)
    c.set("h2", {"results": [], "_q": "b"}, ttl=60)
    assert c.invalidate() == 2
    assert c.stats()["rows"] == 0
    assert c.invalidate() == 0
    c.close()


def test_delete(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "a"}, ttl=60)
    assert c.delete("h1") is True
    assert c.delete("h1") is False
    c.close()


def test_fuzzy_query_ids_typo(tmp_path):
    try:
        import rapidfuzz  # noqa: F401
    except ImportError:
        return
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "python"}, ttl=60)
    ids = c._fuzzy_query_ids("pyton")
    assert "h1" in ids
    rows = c.list_rows(q="pyton")
    assert "h1" in {r["hash"] for r in rows}
    c.close()


def test_stats_keys(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    c.set("h1", {"results": [], "_q": "a"}, ttl=60)
    stats = c.stats()
    for key in ("rows", "unexpired_rows", "db_size_bytes", "total_hits"):
        assert key in stats
    assert stats["rows"] == 1
    assert stats["unexpired_rows"] == 1
    assert stats["db_size_bytes"] > 0
    c.close()
