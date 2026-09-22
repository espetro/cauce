# later: SearXNG format=json compat shim
Issue: #121

Seam: `oxe-server` routes — a `/search?format=json` param-alias plus a SearXNG-shape
serializer (~100-150 LoC), and a POST arm for LangChain JS.
Trigger: adoption pull from the SearXNG client ecosystem (LangChain `SearxSearchWrapper`
GETs `/search&format=json`, Open WebUI, searxng-python clients — all consume JSON only).
Shape: the caller-facing diff is documented: `pageno→page`, `language→lang`,
`content←snippet`, `publishedDate←published`, `engines:[engine]`, `category:"general"`,
`number_of_results: results.len()`, `unresponsive_engines` from `meta.engines_used`
failed entries, and 502 `upstream_failed` must map to 200-empty for drop-in parity. The
shim deliberately relaxes the strict unknown-param 400 only inside the shim arm.
