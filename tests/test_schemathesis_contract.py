"""Property-based contract test: does the live app actually honor the spec
it publishes?

``tests/test_openapi_coverage.py`` checks the spec is *complete* (every
operation typed both ways); this test checks the running app is
*consistent* with that spec -- payloads the handler accepts but the spec
forbids, and vice versa (the schemathesis row in the plan's Python gates
table).

Runs entirely in-process over ASGI (``schemathesis.openapi.from_asgi``), no
real network or server process: ``app.state.search_service`` is swapped for
a ``FakeEngine``-backed ``SearchService`` (same pattern as
``tests/search/test_service.py``) before the schema is built, so
schemathesis's randomly-generated query text never reaches DuckDuckGo.
``max_examples`` is capped low so this runs in seconds as part of the
ordinary `test:py`/`validate` gate, not as a separate slow suite.
"""

import schemathesis
from schemathesis.config._checks import ChecksConfig, SimpleCheckConfig

from oxe.app import create_app
from oxe.cache import TTLCache
from oxe.config import cache_db_path
from oxe.search.model import SearchRequest, SearchResult, SearxResponse
from oxe.search.service import SearchService

_MAX_EXAMPLES = 20


class _FakeEngine:
    """A SearchEngine returning a canned response, doing no I/O."""

    def __init__(self) -> None:
        self.name = "fake"
        self.timeout = 10.0

    async def search(self, req: SearchRequest) -> SearxResponse:
        result = SearchResult(url="https://example.com", title=req.q or "result", engine="fake")
        return SearxResponse(query=req.q, number_of_results=1, results=[result])


def _build_app_for_contract_testing() -> object:
    app = create_app()
    app.state.search_service = SearchService(_FakeEngine(), TTLCache(cache_db_path()))
    return app


schema = schemathesis.openapi.from_asgi("/openapi.json", _build_app_for_contract_testing())
schema.config.generation.max_examples = _MAX_EXAMPLES
# Two checks disabled as out of scope for this app's hand-written response
# models: `status_code_conformance` flags FastAPI/Starlette's built-in 400
# for a malformed (non-UTF8) request body, which happens before any route
# handler or declared response runs, so there's no `response_model` that
# could ever document it; `allow_header_conformance` flags the `Allow`
# header FastAPI's default `OPTIONS`/405 handling returns (e.g. `GET
# /search` OPTIONS-ing to a 405 with no `Allow: POST`), framework behavior
# this app never configures. Both are framework-level, not part of the
# oxe.search.model / oxe.api.* wire contract this test exists to check.
schema.config.checks = ChecksConfig(
    status_code_conformance=SimpleCheckConfig(enabled=False),
    allow_header_conformance=SimpleCheckConfig(enabled=False),
)


# schemathesis ships `py.typed` but `Case` is generic and its own
# `parametrize()`/`call_and_validate()` signatures resolve to partially
# `Unknown` types internally (not a schema boundary this repo controls, and
# not fixable by narrowing an argument the way `oxe.config._as_object_dict`
# narrows `tomllib`'s untyped return) -- basedpyright strict flags every one
# of those as `reportUnknown*`/`reportMissingTypeArgument`, suppressed here
# rather than papered over with a local stub for schemathesis's full surface.
@schema.parametrize()  # pyright: ignore[reportUnknownMemberType, reportUntypedFunctionDecorator]
def test_api_matches_its_own_spec(
    case: schemathesis.Case,  # pyright: ignore[reportUnknownParameterType, reportMissingTypeArgument]
) -> None:
    case.call_and_validate()  # pyright: ignore[reportUnknownMemberType]
