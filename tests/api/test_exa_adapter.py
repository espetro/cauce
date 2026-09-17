"""Round-trip property tests for ``oxe.api.exa``'s two pure functions.

No live engine: a ``SearxResponse`` is synthesized directly (arbitrary but
valid ``SearchResult`` rows) rather than obtained from a real
``SearchService.search()`` call, matching the plan's "you don't need a live
engine" guidance. These tests exercise ``exa_request_to_query`` and
``searx_response_to_exa`` purely as functions of their inputs, pinning the
three adapter rules from the plan's "search contract" section that are
logic rather than shape.
"""

import string

from hypothesis import given
from hypothesis import strategies as st

from oxe.api.exa import ExaContents, ExaSearchRequest, exa_request_to_query, searx_response_to_exa
from oxe.search.model import SearchRequest, SearchResult, SearxResponse

_DOMAIN = st.text(alphabet=string.ascii_lowercase, min_size=1, max_size=10).map(
    lambda s: f"{s}.com"
)

exa_requests = st.builds(
    ExaSearchRequest,
    query=st.text(alphabet=string.printable, min_size=1, max_size=40).map(str.strip).filter(bool),
    type=st.sampled_from(["auto", "instant"]),
    num_results=st.integers(min_value=1, max_value=30),
    page=st.integers(min_value=1, max_value=20),
    category=st.sampled_from(["", "news", "other"]),
    include_domains=st.lists(_DOMAIN, max_size=3),
    exclude_domains=st.lists(_DOMAIN, max_size=3),
    contents=st.builds(ExaContents, text=st.booleans(), highlights=st.booleans()),
)

search_results = st.builds(
    SearchResult,
    url=st.text(alphabet=string.ascii_letters, min_size=1, max_size=20).map(
        lambda s: f"https://example.com/{s}"
    ),
    title=st.text(max_size=40),
    content=st.text(max_size=200),
    engine=st.just("ddg"),
    published_date=st.none(),
    thumbnail=st.none(),
)


@given(req=exa_requests)
def test_exa_request_to_query_never_raises_and_carries_query_text(req: ExaSearchRequest) -> None:
    out = exa_request_to_query(req)

    assert isinstance(out, SearchRequest)
    assert req.query.strip() in out.q
    assert out.pageno == req.page


@given(req=exa_requests)
def test_exa_request_to_query_news_category_maps_to_news(req: ExaSearchRequest) -> None:
    out = exa_request_to_query(req)

    assert out.categories == (["news"] if req.category == "news" else ["general"])


@given(req=exa_requests, results=st.lists(search_results, max_size=40))
def test_num_results_slicing_is_respected(
    req: ExaSearchRequest, results: list[SearchResult]
) -> None:
    resp = SearxResponse(query=req.query, number_of_results=len(results), results=results)

    out = searx_response_to_exa(resp, req)

    assert len(out.results) <= req.num_results
    assert len(out.results) == min(len(results), req.num_results)
    assert [r.url for r in out.results] == [r.url for r in results[: req.num_results]]


@given(req=exa_requests, results=st.lists(search_results, max_size=10))
def test_searx_response_to_exa_never_raises_on_valid_input(
    req: ExaSearchRequest, results: list[SearchResult]
) -> None:
    resp = SearxResponse(query=req.query, number_of_results=len(results), results=results)

    out = searx_response_to_exa(resp, req)

    assert out.search_type == req.type
    for item, src in zip(out.results, results[: req.num_results], strict=True):
        assert item.url == src.url
        assert item.id == src.url
        assert item.text == (src.content if req.contents.text else "")
