"""Freshness check: committed openapi.json must match app.openapi().

If this fails, the API contract changed: run `mise run openapi` and commit
the regenerated openapi.json together with the change. Also covers the
SearchResponse `_`-field round-trip guarantee.
"""

import json
from pathlib import Path

from fastapi.testclient import TestClient

from oxe import exa_compat
from oxe.cache import TTLCache
from oxe.server import make_app
from oxe.server.schemas import SearchResponse

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "openapi.json"


def test_openapi_is_fresh():
    committed = json.loads(SPEC.read_text())
    current = make_app().openapi()
    if committed != current:
        committed_paths = set(committed.get("paths", {}))
        current_paths = set(current.get("paths", {}))
        added = current_paths - committed_paths
        removed = committed_paths - current_paths
        changed = {
            p
            for p in committed_paths & current_paths
            if committed["paths"][p] != current["paths"].get(p)
        }
        committed_models = set(committed.get("components", {}).get("schemas", {}))
        current_models = set(current.get("components", {}).get("schemas", {}))
        drift = []
        if added:
            drift.append(f"added paths: {sorted(added)}")
        if removed:
            drift.append(f"removed paths: {sorted(removed)}")
        if changed:
            drift.append(f"changed paths: {sorted(changed)}")
        if committed_models != current_models:
            drift.append(
                f"schemas changed: +{sorted(current_models - committed_models)} "
                f"-{sorted(committed_models - current_models)}"
            )
        raise AssertionError(
            "openapi.json is stale; regenerate with `mise run openapi`. Drift: "
            + "; ".join(drift)
        )


def test_search_response_underscore_fields_round_trip():
    """SearchResponse serializes _-prefixed cache-transparency fields verbatim."""
    payload = {
        "requestId": "r",
        "searchType": "auto",
        "results": [],
        "_page": 1,
        "costDollars": {"total": 0.0},
        "_source": "cache",
        "_q_hash": "abc",
        "_q": "hello",
        "_backend": "ddg",
        "_duration_ms": 42,
        "_cached_at": 1700000000,
        "_error": None,
        "_error_kind": None,
    }
    out = SearchResponse.model_validate(payload).model_dump(by_alias=True)
    for k, v in payload.items():
        assert out[k] == v, k

    # end-to-end through the JSON branch of GET /search (seeded cache hit)
    cache = TTLCache(":memory:")
    req = {
        "query": "hello",
        "numResults": 10,
        "page": 1,
        "contents": {"text": True, "highlights": True},
    }
    key = exa_compat.cache_key(req | {"_backend": "ddg"})
    cache.set(
        key,
        {"_q": "hello", "results": [{"title": "t", "url": "https://a.example"}]},
        ttl=3600,
    )
    app = make_app(cache=cache, backend=object())
    client = TestClient(app)
    r = client.get("/search", params={"q": "hello"}, headers={"accept": "application/json"})
    assert r.status_code == 200
    body = r.json()
    assert body["_source"] == "cache"
    assert body["_q"] == "hello"
    assert body["_q_hash"] == key
    assert "_cached_at" in body
