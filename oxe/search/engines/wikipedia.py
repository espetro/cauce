"""Wikipedia Opensearch engine: a keyless, low-rate-limit dev/test backend.

Calls MediaWiki's ``action=opensearch`` endpoint directly over stdlib
``urllib`` (same pattern as ``oxe/ai.py``'s ``_chat_completion``: a blocking
call wrapped in ``asyncio.to_thread``, no new runtime dependency).

Why this exists: the ``"wikipedia"`` entry in ``ddgs.DDGS_ENGINES`` is not a
real alternative to DuckDuckGo. It still routes through ``ddgs``'s shared
randomized-fingerprint HTTP client and sends no descriptive User-Agent, so
it hits the same anti-bot surface. This engine sends a descriptive
User-Agent per Wikipedia's API etiquette, which is what keeps it well under
the rate limits. Both entries coexist: this one registers as
``"wikipedia-opensearch"``.

Scope: realistic-but-narrow encyclopedic data for development and tests. It
is not a substitute for general web search in production.

Request fields Opensearch has no equivalent for are ignored, never raised
on, so the engine can sit inside a fallback/fanout composition next to
engines that do honor them:

- ``pageno``: Opensearch returns one page (the top ``limit`` titles) with no
  offset parameter.
- ``categories``: Opensearch is title-prefix search over one namespace
  (articles); there are no result categories.
- ``time_range``: pages carry no recency filter in this API.
- ``safesearch``: Wikipedia has no such switch.

``language`` is honored by selecting the Wikipedia language subdomain
(``fr.wikipedia.org``); ``"all"`` and anything unparseable use English.
"""

import asyncio
import json
import re
import urllib.error
import urllib.parse
import urllib.request
from typing import Final, cast

from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearchResult, SearxResponse

_DEFAULT_TIMEOUT_S: Final = 10.0
_DEFAULT_LANG: Final = "en"
_LIMIT: Final = 10
_USER_AGENT: Final = (
    "oxe/0.5 (https://github.com/espetro/oxe; local search test backend) python-urllib"
)
# Wikipedia language subdomains are 2-3 lowercase letters (with the odd
# hyphenated code, e.g. ``zh-yue``). Validating keeps arbitrary request
# input from steering the request to another host.
_LANG_RE: Final = re.compile(r"^[a-z]{2,3}(-[a-z]{2,8})?$")


def _lang_for(language: str) -> str:
    """SearXNG language code (``en-US``, ``fr``, ``all``) -> Wikipedia subdomain."""
    if not language or language == "all":
        return _DEFAULT_LANG
    lang = language.lower().replace("_", "-").partition("-")[0]
    return lang if _LANG_RE.match(lang) else _DEFAULT_LANG


def _build_url(query: str, lang: str) -> str:
    params = urllib.parse.urlencode(
        {
            "action": "opensearch",
            "search": query,
            "limit": _LIMIT,
            "namespace": 0,
            "format": "json",
        }
    )
    return f"https://{lang}.wikipedia.org/w/api.php?{params}"


def _fetch(url: str, timeout: float) -> object:
    """Blocking GET returning the decoded JSON body. Always run via ``asyncio.to_thread``."""
    request = urllib.request.Request(  # noqa: S310 - https only, host built from a validated lang
        url,
        headers={"User-Agent": _USER_AGENT, "Accept": "application/json"},
        method="GET",
    )
    with urllib.request.urlopen(request, timeout=timeout) as resp:  # noqa: S310
        return cast(object, json.loads(resp.read()))


def _str_list(value: object) -> list[str]:
    if not isinstance(value, list):
        return []
    items = cast(list[object], value)
    return [i if isinstance(i, str) else "" for i in items]


def _parse(body: object, *, engine_name: str) -> list[SearchResult]:
    """Zip ``[term, [titles], [descriptions], [urls]]`` into canonical results."""
    if not isinstance(body, list):
        msg = f"{engine_name}: unexpected opensearch payload shape"
        raise BackendError(msg)
    parts = cast(list[object], body)
    if len(parts) < 4:  # noqa: PLR2004 - the four-element opensearch tuple
        msg = f"{engine_name}: unexpected opensearch payload shape"
        raise BackendError(msg)
    titles, descriptions, urls = _str_list(parts[1]), _str_list(parts[2]), _str_list(parts[3])
    return [
        SearchResult(
            url=url,
            title=title,
            content=descriptions[i] if i < len(descriptions) else "",
            engine=engine_name,
            engines=[engine_name],
        )
        for i, (title, url) in enumerate(zip(titles, urls, strict=False))
    ]


class WikipediaEngine:
    """Wikipedia Opensearch backend implementing the ``SearchEngine`` protocol."""

    name = "wikipedia-opensearch"

    def __init__(self) -> None:
        self.timeout = _DEFAULT_TIMEOUT_S

    async def search(self, req: SearchRequest) -> SearxResponse:
        url = _build_url(req.q, _lang_for(req.language))
        try:
            body = await asyncio.to_thread(_fetch, url, self.timeout)
        except urllib.error.HTTPError as e:
            msg = f"{self.name}: HTTP {e.code} from wikipedia"
            raise BackendError(msg) from e
        except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as e:
            msg = f"{self.name}: request failed: {e}"
            raise BackendError(msg) from e
        results = _parse(body, engine_name=self.name)
        return SearxResponse(query=req.q, number_of_results=len(results), results=results)
