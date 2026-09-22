# later: QuickJS scripted engine runtime
Issue: #65

Seam: `cauce-engines` `Engine` trait; a fourth runtime next to declarative/exec/replay.
Trigger: a shipped engine needs token signing, multi-step cookies or client-side computed
params that the declarative YAML cannot express and the exec bridge is too heavy for a router.
Shape: `rquickjs`, one context per engine, `scrape(req, http) -> results` exported from
`engines/<id>.js`, same `EngineError` mapping, same fixture-based `cauce engine test`.
Must not change: `Engine` trait, `SearchResult`, politeness/egress path (JS gets a bound
`http.get` that goes through `HttpClient`).
