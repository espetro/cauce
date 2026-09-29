# Report bundle machinery (#238 + #239 + #241)

- `cauce_core::report` is axum-free and returns `serde_json::Value` payloads; the
  envelope (`v`, `profile`, `generated_at`, `notes`) + `#[serde(flatten)]
  sections: BTreeMap<String, Value>` gives the documented wire shape AND the
  register-anything seam (a future agent crate section joins without touching
  the collector). BTreeMap also makes wire order stable = alphabetical.
- `SECRET_PATHS` in `config::redact` covers only `ai.api_key` + `engines.<i>.env.*`
  — NOT `egress.proxy`. Proxy credentials ride the report's URL-userinfo scrub
  instead; under `safe` the `?` component goes too (`engine_http` spans log the
  full upstream URL incl. `?q=` — a URL query leaf IS a query leaf).
- Query fold: `query`/`query_raw` leaves become `query_hash` = sha256 over the
  `push_str`-framed `normalize_query` (CacheKey *shape*, not a real CacheKey —
  real ones hash query+page+lang+… so a bare query can't reproduce them). Drop
  `query` when a sibling `query_hash` already exists (search_log rows).
- Registry = `LazyLock<RwLock<Vec<Arc<dyn _>>>>` like `metrics.rs`, dedupe by
  name (last wins → re-registration is idempotent), `RESERVED_KEYS` panic.
  Sinks are `catch_unwind`-guarded: a panicking sink warns, never breaks the
  producer path (`audit()` emits its `ReportEvent::Audit` synchronously).
- Collector serialization: `tokio::sync::Mutex` in `cauce_server::report::collect`
  — registered sections capture `AppState`, so exports must not interleave
  (also keeps tests with different states honest on the shared registry).
- `await_holding_lock` lint vs process-global registries: in tests use an
  `#[allow]` with a comment, or `tokio::sync::Mutex` in prod code — never
  `std::sync::Mutex` across `.await`.
- `LogRecord`/`SpanPart` needed `Serialize` for `errors_tail` — the JSONL writer
  emits `level` as tracing's `Level::as_str()` (uppercase); filter warn|error
  case-insensitively.
- `engine_views` is `pub(crate)` at `handlers::` — the collector lives in the
  same crate, so the seam stays axum-free without re-exporting internals.
