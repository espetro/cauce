# oxe-engine-sdk (Python)

Python SDK for the oxe exec-engine protocol plus `ddgs_auto.py`, the
reference engine that bridges DuckDuckGo while native engine specs land.

Protocol v1 is one JSON object per line on stdio, in each direction:

```
-> {"v":1,"query":"tail tolerance rust","page":1,"lang":"en","timeout_ms":1500}
<- {"v":1,"results":[{"title":"...","url":"...","snippet":"..."}],"error":null}
```

`oxe_engine_sdk.run(fn)` reads `Request`s from stdin until EOF and writes
one `Response` line each. Malformed lines and handler exceptions become
error responses instead of crashes. `error` is a short snake_case code
(`rate_limited`, `blocked`, `no_results`, `parse:<msg>`,
`transport:<msg>`). Zero runtime dependencies: stdlib only, so the SDK
runs under a bare `python3`.

## Writing an engine

```python
from oxe_engine_sdk import Request, Response, Result, run

def search(req: Request) -> Response:
    hits = my_backend(req.query, page=req.page)
    return Response(results=[
        Result(title=h.title, url=h.url, snippet=h.snippet)
        for h in hits if h.url
    ])

if __name__ == "__main__":
    run(search)
```

Register it in `config.toml`:

```toml
[[engines]]
id = "myengine"
kind = "exec"
command = "python3"
args = ["/path/to/myengine.py"]
```

## The ddgs reference engine

`oxe_engine_sdk/ddgs_auto.py` runs `DDGS().text(..., backend="auto")` over
the protocol and is the built-in `ddgs` config entry (tier 2, enabled).
It needs the `ddgs` extra:

```bash
cd sdk/python
uv sync --extra ddgs
```

## Iterating without burning rate limits

While developing an engine, the UI, or client wiring, do not hammer the
web engines. Two engines exist exactly for this:

- `wikipedia` (`engines/wikipedia.yaml`): the MediaWiki OpenSearch API.
  Keyless, JSON, and gentle on rate limits, so it is the engine to pin
  for real network results: `engines=["wikipedia"]` on the MCP
  `search_web` tool or `engines=wikipedia` on `/api/search`. The spec
  ships `enabled: false`, so add the entry first (this also puts it in
  the default fan-out):

  ```toml
  [[engines]]
  id = "wikipedia"
  kind = "declarative"
  ```

- `replay`: deterministic synthetic results, fully offline. It is a
  built-in entry that ships disabled; enable it for a run with
  `OXE_ENGINES=replay` (what the golden-path tests do) or pin
  `engines=["replay"]` once enabled.

Try either spec directly without a server:

```bash
oxe engine test engines/wikipedia.yaml                # recorded fixtures
oxe engine test engines/wikipedia.yaml --live "rust"  # one real fetch
```
