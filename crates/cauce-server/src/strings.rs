//! User-visible copy for the HTMX pages.
//!
//! The wave-2 settled inputs require all page strings to live here so a
//! later i18n pass is a mechanical change, not a rewrite. `common` holds
//! strings shared across pages; each page gets one `pub mod <page>` of
//! flat `pub const` items, referenced as `crate::strings::cache::TITLE`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// Strings shared across pages.
pub mod common {
    /// The product name (brand link, title suffix).
    pub const BRAND: &str = "cauce";
    /// Empty-cell placeholder.
    pub const DASH: &str = "-";
    /// Footer label ahead of the page's own request id.
    pub const REQUEST_LABEL: &str = "request";

    /// Primary nav link to the search landing page.
    pub const NAV_SEARCH: &str = "search";
    /// Primary nav link to `/history`.
    pub const NAV_HISTORY: &str = "history";
    /// Primary nav link to `/dashboard`.
    pub const NAV_DASHBOARD: &str = "dashboard";
    /// Operator nav link to `/engines`.
    pub const NAV_ENGINES: &str = "engines";
    /// Operator nav link to `/cache`.
    pub const NAV_CACHE: &str = "cache";
    /// Operator nav link to `/audit`.
    pub const NAV_AUDIT: &str = "audit";
    /// Header link to `/settings` (outside the operator group).
    pub const NAV_SETTINGS: &str = "settings";
    /// Label of the collapsed operator-group menu below 700 px.
    pub const NAV_MORE: &str = "more";
    /// `aria-label` of the primary nav landmark.
    pub const NAV_PRIMARY_LABEL: &str = "primary";
    /// `aria-label` of the operator nav landmark (inline and inside `more`).
    pub const NAV_OPERATOR_LABEL: &str = "operator";

    /// Theme toggle button text and `aria-label`; cycles system -> light
    /// -> dark (W2-08). JS rewrites the text to the active state's word.
    pub const THEME_SWITCH: &str = "theme";
    /// Theme state word shown on the toggle while following
    /// `prefers-color-scheme` (no stored choice).
    pub const THEME_SYSTEM: &str = "system";
    /// Theme state word while light is forced.
    pub const THEME_LIGHT: &str = "light";
    /// Theme state word while dark is forced.
    pub const THEME_DARK: &str = "dark";
    /// `aria-label` for the toggle before JS runs; names the cycle it
    /// steps through. Once wired, `THEME_ARIA_STATE` takes over so the
    /// accessible name announces the active state.
    pub const THEME_ARIA: &str = "theme: cycles system, light, dark";
    /// `aria-label` template the toggle JS fills in on every state change
    /// (`theme: dark`); `{state}` is replaced with the state's word.
    pub const THEME_ARIA_STATE: &str = "theme: {state}";
}

/// `/` and `/search` page copy: the landing form, the server-rendered
/// results page and the W2-01 streaming shell. The streaming page's
/// inline JS interpolates these too (serialized once into the page as a
/// `var S = {...}` literal), so no user-visible string lives in the
/// template or the script.
pub mod search {
    /// Search-box placeholder.
    pub const PLACEHOLDER: &str = "Search...";
    /// Search-box submit label.
    pub const SUBMIT: &str = "Search";
    /// Result-count suffix (`12 results`).
    pub const RESULTS: &str = "results";
    /// Empty-state lead-in (`No results` / `No results · <statuses>`).
    pub const NO_RESULTS: &str = "No results";
    /// Next-page button label.
    pub const MORE: &str = "More";

    /// Badge while the SSE stream is in flight.
    pub const SEARCHING: &str = "searching...";
    /// Status line under the stream shell before the first event.
    pub const WAITING: &str = "Waiting for engines...";
    /// Status line once the terminal `meta` event lands.
    pub const COMPLETE: &str = "Search complete";
    /// Status line when an SSE frame fails to parse.
    pub const INVALID_STREAM: &str = "Search stream returned invalid data";
    /// `<noscript>` on the streaming shell: the events need JavaScript,
    /// while a plain form submit still runs a server-rendered search.
    pub const NOSCRIPT_STREAM: &str =
        "Streaming needs JavaScript; submit the form for a plain search.";

    /// Outranking pill (`{n}` is the late-result count).
    pub const NEW_ABOVE: &str = "{n} new results above";
    /// Live (network) badge (`{ms}` is the elapsed time in ms).
    pub const LIVE_BADGE: &str = "live · {ms} ms";
    /// Cache badge (`{age}`/`{ttl}` in seconds).
    pub const CACHED_BADGE: &str = "cached · {age} s ago · ttl {ttl} s";
    /// JS-side cache lead-in (the stream only knows `source != network`).
    pub const CACHED: &str = "cached";
    /// Stale-serve badge, server-rendered and streamed (W3-02).
    pub const STALE_BADGE: &str = "stale · refreshing";

    /// Engine status phrase (`{kind}` is an `ERR_*` word).
    pub const ENGINE_FAILED: &str = "{engine} failed ({kind})";
    /// Breaker-skipped engine phrase.
    pub const ENGINE_SKIPPED: &str = "{engine} skipped (breaker)";

    /// `EngineError::RateLimited` kind word.
    pub const ERR_RATE_LIMITED: &str = "rate limited";
    /// `EngineError::Blocked` kind word.
    pub const ERR_BLOCKED: &str = "blocked";
    /// `EngineError::Timeout` kind word.
    pub const ERR_TIMEOUT: &str = "timeout";
    /// `EngineError::Parse` kind word.
    pub const ERR_PARSE: &str = "parse";
    /// `EngineError::Transport` kind word.
    pub const ERR_TRANSPORT: &str = "transport";
    /// `EngineError::NoResults` kind word.
    pub const ERR_NO_RESULTS: &str = "no results";
    /// Kind word when the wire payload names none.
    pub const ERR_UNKNOWN: &str = "failed";
}

/// `/cache` page copy (W2-04).
pub mod cache {
    pub const PAGE_TITLE: &str = "Cache";
    /// Search-box placeholder.
    pub const FILTER_PLACEHOLDER: &str = "Filter cached queries...";
    /// Search-box submit label.
    pub const FILTER_BUTTON: &str = "Filter";
    /// "Clear filter" link shown while `q` is active.
    pub const FILTER_CLEAR: &str = "clear";
    /// Bulk action: delete rows past `expires_at`.
    pub const DELETE_EXPIRED: &str = "Delete expired";
    /// Bulk action: delete every row.
    pub const DELETE_ALL: &str = "Delete all";
    /// `hx-confirm` for the expired-rows bulk delete.
    pub const CONFIRM_EXPIRED: &str = "Delete all expired entries?";
    /// `hx-confirm` for the full-table delete.
    pub const CONFIRM_ALL: &str =
        "Delete every cached entry? Searches will hit the network until the cache refills.";
    /// `hx-confirm` for a single-row delete.
    pub const CONFIRM_ROW: &str = "Delete this cache entry?";
    /// Per-row delete button label.
    pub const DELETE_ROW: &str = "Delete";
    /// Empty state when the table has no rows at all.
    pub const EMPTY: &str = "no cached queries yet. run a search and it lands here.";
    /// Empty state when `q` matched nothing; `{q}` is the active filter.
    pub const EMPTY_FILTERED: &str = "nothing cached matches \"{q}\".";
    /// Pagination: link to the previous (newer) page.
    pub const PAGE_PREV: &str = "Newer";
    /// Pagination: link to the next (older) page.
    pub const PAGE_NEXT: &str = "Older";
    /// Note shown in filtered mode, which is capped at one page; `{n}` is
    /// the row cap.
    pub const FILTERED_CAP: &str = "showing the newest {n} matches";
    /// Placeholder inside the payload slot before the lazy fragment loads.
    pub const PAYLOAD_LOADING: &str = "loading...";
    /// Inline error inside the payload slot when the lazy fetch fails;
    /// `{status}` is the HTTP status code.
    pub const PAYLOAD_ERROR: &str = "error: could not load payload ({status})";
    /// `<noscript>` hint next to the delete controls: deletes go through
    /// htmx, so they do nothing without JavaScript.
    pub const NOSCRIPT_DELETES: &str = "the delete buttons need JavaScript.";
    /// Count line, singular (`1 entry`).
    pub const ENTRY_ONE: &str = "entry";
    /// Count line, plural (`34 entries`).
    pub const ENTRY_MANY: &str = "entries";
    /// Count line qualifier while a `q` filter is active
    /// (`3 matching entries`).
    pub const MATCHING: &str = "matching";
    /// Hit count, singular (`1 hit`).
    pub const HIT_ONE: &str = "hit";
    /// Hit count, plural (`3 hits`).
    pub const HIT_MANY: &str = "hits";
    /// Row expiry phrase while the entry is live; `{rel}` is a relative
    /// duration like `41m`.
    pub const EXPIRES_IN: &str = "expires in {rel}";
    /// Row expiry phrase once `expires_at` has passed.
    pub const EXPIRED_AGO: &str = "expired {rel} ago";
}

pub mod settings {
    pub const PAGE_TITLE: &str = "Settings";
    pub const EDITING: &str = "editing";
    pub const RESTART_NOTE: &str = "changes apply on restart";

    pub const SECTION_SEARCH: &str = "Search";
    pub const DEADLINE: &str = "Deadline (ms)";
    pub const TTL: &str = "Cache TTL (s)";
    pub const MIN_RESULTS: &str = "Min results before hedge";
    pub const HEDGE_FLOOR: &str = "Hedge floor (ms)";
    pub const HEDGE_CEILING: &str = "Hedge ceiling (ms)";

    pub const SECTION_ENGINES: &str = "Engines";
    pub const ENGINES_PINNED: &str = "enabled flags are pinned by CAUCE_ENGINES";
    pub const ENABLED: &str = "enabled";
    pub const DISABLED: &str = "disabled";
    pub const TIER: &str = "tier";
    pub const TIER_DEFAULT: &str = "default";
    pub const PROXY: &str = "egress proxy";
    pub const PROXY_PLACEHOLDER: &str = "direct";
    pub const BROWSE_ENGINES: &str = "browse engines";

    pub const SECTION_ADMISSION: &str = "Admission";
    pub const MAX_WAIT: &str = "Max queue wait (ms)";
    pub const MAX_CONCURRENT: &str = "Max concurrent per engine";

    pub const SECTION_LOGGING: &str = "Logging";
    pub const RETENTION: &str = "JSONL retention (days)";

    pub const SECTION_CACHE: &str = "Cache";
    pub const ENTRIES: &str = "entries";
    pub const UNEXPIRED: &str = "unexpired";
    pub const NEWEST: &str = "newest";
    pub const DELETE_EXPIRED: &str = "delete expired";
    pub const DELETE_ALL: &str = "delete all";
    pub const CONFIRM_EXPIRED: &str = "Delete every expired cache entry?";
    pub const CONFIRM_ALL: &str =
        "Delete ALL cache entries? Every next search will hit the network.";
    pub const BROWSE_ENTRIES: &str = "browse entries";

    pub const SECTION_AI: &str = "AI answers";
    pub const AI_BASE_URL: &str = "Base URL";
    pub const AI_API_KEY: &str = "API key";
    pub const AI_API_KEY_PLACEHOLDER: &str = "${env:CAUCE_AI_API_KEY}";
    pub const AI_MODEL: &str = "Model";
    pub const AI_MODEL_PLACEHOLDER: &str = "model name";
    pub const MODELS_UNREACHABLE: &str = "model list unreachable; type a model name";
    pub const AI_PIPELINE_NOTE: &str = "(applies on restart)";

    pub const SET_BY: &str = "set by";
    pub const IS_SET: &str = "is set";
    pub const IS_NOT_SET: &str = "is not set";

    pub const SAVE: &str = "Save";
    pub const SAVED: &str = "saved";
    pub const NOT_SAVED: &str = "not saved:";
    pub const ERROR_ONE: &str = "error";
    pub const ERROR_MANY: &str = "errors";
    pub const COULD_NOT_SAVE: &str = "error: could not save";
    pub const NOSCRIPT: &str =
        "Saving needs JavaScript (the form issues PUT /api/config via htmx).";
}

/// `/history` page copy (W2-02).
pub mod history {
    pub const TITLE: &str = "History";
    /// Stats line: `N searches in last 24h · N total · N clicks today`.
    pub const STAT_SEARCHES_24H: &str = "searches in last 24h";
    pub const STAT_TOTAL: &str = "total";
    pub const STAT_CLICKS_TODAY: &str = "clicks today";
    pub const FILTER_SUBMIT: &str = "Filter";
    pub const WINDOW_24H: &str = "last 24h";
    pub const WINDOW_7D: &str = "last 7d";
    pub const WINDOW_30D: &str = "last 30d";
    pub const WINDOW_ALL: &str = "all time";
    /// `aria-label` for the `since` select; the control renders no
    /// visible label, so the accessible name comes from the attribute.
    pub const SINCE_LABEL: &str = "time window";
    /// `aria-label` for the `q` input; the placeholder is not an
    /// accessible name.
    pub const QUERY_LABEL: &str = "filter by query text";
    pub const QUERY_PLACEHOLDER: &str = "filter query text...";
    pub const CACHED_ONLY: &str = "cached only";
    pub const CLEAR: &str = "clear";
    pub const EMPTY: &str = "nothing searched yet. run a search and it lands here.";
    /// Filtered-empty fragments, composed as
    /// `no searches match "q" in the last 7 days that are still cached.`
    pub const EF_MATCH: &str = "no searches match";
    pub const EF_NONE: &str = "no searches";
    pub const IN_24H: &str = "in the last 24h";
    pub const IN_7D: &str = "in the last 7 days";
    pub const IN_30D: &str = "in the last 30 days";
    pub const IN_SINCE: &str = "since";
    pub const EF_CACHED: &str = "that are still cached";
    /// Cap note: `showing 200 of N · use the filters to reach older searches`.
    pub const CAPPED_SHOWING: &str = "showing";
    pub const CAPPED_OF: &str = "of";
    pub const CAPPED_HINT: &str = "use the filters to reach older searches";
    pub const COL_QUERY: &str = "query";
    pub const COL_WHEN: &str = "when";
    pub const COL_SOURCE: &str = "source";
    pub const COL_ENGINES: &str = "engines";
    pub const COL_RESULTS: &str = "n";
    pub const COL_LATENCY: &str = "ms";
    pub const COL_CLIENT: &str = "by";
    /// Source-cell fragments: `cached · 41m`, `cached · expired`,
    /// `network · t1`.
    pub const SRC_CACHED: &str = "cached";
    pub const SRC_EXPIRED: &str = "expired";
    pub const SRC_NETWORK: &str = "network";
    /// `t` in `network · t1`.
    pub const TIER_PREFIX: &str = "t";
    /// `#` in a click's `#position` marker.
    pub const POSITION_PREFIX: &str = "#";
    pub const CLICKS_WORD: &str = "clicks";
    /// Singular of `CLICKS_WORD` for the delete confirm.
    pub const CLICK_ONE: &str = "click";
    pub const CLICK_ONLY: &str = "(click only)";
    pub const RERUN: &str = "re-run";
    pub const COPY_JSON: &str = "copy json";
    pub const COPIED: &str = "copied";
    pub const PAYLOAD: &str = "payload";
    pub const DELETE: &str = "delete";
    pub const DELETE_CONFIRM: &str = "Delete this search?";
    /// `hx-confirm` prefix when the row has nested clicks, composed as
    /// `Delete this search and its {n} {click|clicks}?`.
    pub const DELETE_CONFIRM_CLICKS_PRE: &str = "Delete this search and its";
    /// Inline error a failed `DELETE /api/history/<id>` writes into the
    /// row (htmx `response-error`); `{status}` is replaced client-side.
    pub const DELETE_FAILED: &str = "error: delete failed ({status})";
    pub const REQUEST_LABEL: &str = "request";
}
/// `/dashboard` (W2-03).
pub mod dashboard {
    pub const TITLE: &str = "dashboard";
    pub const WINDOW: &str = "window";
    pub const DAYS_7: &str = "7 days";
    pub const DAYS_30: &str = "30 days";
    /// Flat muted placeholder for panels whose source table has no rows in
    /// the window (the screen spec's empty state).
    pub const NO_DATA: &str = "no search log data yet";
    pub const NO_ENGINES: &str = "no engine data yet";

    pub const SEARCHES_PER_DAY: &str = "searches per day";
    pub const LEGEND_CACHE: &str = "cache";
    pub const LEGEND_NETWORK: &str = "network";
    pub const HIT_RATE: &str = "cache hit rate";
    pub const LATENCY: &str = "latency";
    pub const TTFR: &str = "time to first result";
    pub const FULL: &str = "full request";
    pub const CLIENTS: &str = "client split";
    pub const OUTCOMES: &str = "request outcomes";
    pub const TOP_QUERIES: &str = "top queries";
    pub const ZERO_RESULTS: &str = "zero-result queries";
    pub const RELIABILITY: &str = "reliability";
    pub const DEADLINE_HITS: &str = "deadline-hit";
    pub const STALE_SERVED: &str = "stale-served";
    pub const ADMISSION_REJECTED: &str = "admission-rejected";
    pub const ENGINES: &str = "engines";
    pub const CACHE: &str = "cache";
    /// W3-05 nightly engine relevance evals panel.
    pub const ENGINE_EVAL: &str = "engine relevance (nightly)";
    /// Empty state when no `evals/results/*-engines.json` exists.
    pub const EVAL_NO_RUN: &str = "no eval run yet";

    pub const COL_ENGINE: &str = "engine";
    pub const COL_BREAKER: &str = "breaker";
    pub const COL_RELIABILITY: &str = "reliability";
    pub const COL_CALLS: &str = "calls";
    pub const COL_TOTAL: &str = "total ms (med/p80/p95)";
    pub const COL_HTTP: &str = "http ms (med/p80/p95)";
    pub const COL_PARSE: &str = "parse ms (med/p80/p95)";

    pub const CACHE_ROWS: &str = "rows";
    pub const CACHE_UNEXPIRED: &str = "unexpired";
    pub const CACHE_DB_SIZE: &str = "db size";
    pub const CACHE_NEWEST: &str = "newest";
}

pub mod audit {
    pub const PAGE_TITLE: &str = "Audit";
    pub const ANY: &str = "any";
    pub const FILTER: &str = "Filter";
    pub const CLEAR: &str = "clear";
    pub const EMPTY: &str =
        "nothing audited yet. deleting a cache row or resetting a breaker writes the first entry.";
    pub const FILTERED_EMPTY_PREFIX: &str = "no audit rows match";
    pub const FILTERED_EMPTY_AND: &str = "and";
    pub const ACTOR_LABEL: &str = "actor";
    pub const ACTION_LABEL: &str = "action";
    pub const WHEN_COLUMN: &str = "when";
    pub const ACTOR_COLUMN: &str = "actor";
    pub const ACTION_COLUMN: &str = "action";
    pub const TARGET_COLUMN: &str = "target";
    pub const REQUEST_COLUMN: &str = "request";
    pub const DETAILS_SUMMARY: &str = "details";
    pub const ROW: &str = "row";
    pub const ROWS: &str = "rows";
    pub const MATCHING: &str = "matching";
    pub const CAP_NOTE_PREFIX: &str = "showing the newest";
    pub const CAP_NOTE_SUFFIX: &str = "raise `limit` (max 1000) for more";
    /// Accessible name of the horizontally scrollable table region.
    pub const TABLE_REGION: &str = "audit table";
}

pub mod trace {
    pub const PAGE_TITLE: &str = "Trace";
    pub const COPY: &str = "copy";
    pub const BACK_TO_AUDIT: &str = "back to audit";
    pub const SPANS_HEADING: &str = "spans";
    pub const MS: &str = "ms";
    pub const RESULTS: &str = "results";
    /// `{days}` is replaced with the configured `logs.retention_days`.
    pub const NO_TRACE: &str = "no trace for this request id. traces are kept for logs.retention_days days (currently {days}).";
    pub const BAD_ID: &str = "that is not a request id";
    /// Accessible name of the horizontally scrollable timeline region.
    pub const TIMELINE_REGION: &str = "request timeline";
    /// Accessible name of one span's expandable raw-fields block.
    pub const SPAN_REGION: &str = "span fields";
}
pub mod engines {
    pub const PAGE_TITLE: &str = "Engines";
    /// Summary line under the heading (`3 configured · 2 enabled`).
    pub const SUMMARY: &str = "{configured} configured · {enabled} enabled";
    /// Appended to SUMMARY only while at least one breaker is open.
    pub const SUMMARY_OPEN: &str = " · {open} breaker open";
    /// Page-level note: toggles persist but the live set is fixed until
    /// restart.
    pub const HINT_RESTART: &str = "enable/disable writes config.toml and takes effect on restart.";
    /// Page-level note while `CAUCE_ENGINES` pins the enabled set.
    pub const HINT_PINNED: &str =
        "CAUCE_ENGINES pins the enabled set; resolved state will not move while it is set.";
    /// Empty state: nothing configured, running, or tracked.
    pub const EMPTY: &str = "No engines configured.";
    /// Card header note for an engine the resolved config does not name.
    pub const NOT_IN_CONFIG: &str = "not in config";
    /// Card header note for an engine absent from the running pipeline.
    pub const NOT_RUNNING: &str = "not running";
    /// Breaker chip, closed state.
    pub const BREAKER_CLOSED: &str = "Closed";
    /// Breaker chip, open state.
    pub const BREAKER_OPEN: &str = "Open";
    /// Breaker chip, half-open state.
    pub const BREAKER_HALF_OPEN: &str = "HalfOpen";
    /// Countdown note next to an open chip; `{rel}` is `42s`-style.
    pub const BREAKER_RETRIES: &str = "retries in {rel}";
    /// Note next to an open chip whose window already elapsed (the lazy
    /// `Open -> HalfOpen` truth: the next call probes).
    pub const BREAKER_ELAPSED: &str = "next call probes";
    /// Note next to a half-open chip.
    pub const BREAKER_PROBING: &str = "probing";
    /// Note next to the chip while the engine is disabled (the mockup's
    /// `[ Closed ] disabled`); the countdown wins when both apply.
    pub const DISABLED: &str = "disabled";
    /// Stats labels (definition-list terms).
    pub const STAT_ENABLED: &str = "enabled";
    pub const STAT_EWMA: &str = "ewma";
    pub const STAT_LAST_OK: &str = "last ok";
    pub const STAT_LAST_ERROR: &str = "last error";
    pub const STAT_P95: &str = "p95";
    pub const STAT_RELIABILITY: &str = "reliability";
    pub const STAT_REQUESTS: &str = "requests today";
    /// `enabled` cell values.
    pub const ENABLED_YES: &str = "yes";
    pub const ENABLED_NO: &str = "no";
    /// `last ok` cell: absolute local time plus relative (`01:02 (9m)`).
    pub const LAST_OK_FMT: &str = "{hhmm} ({rel})";
    /// `p95`/`ewma` cell for a real sub-millisecond sample (renders
    /// instead of a misleading `0 ms`).
    pub const SUB_MS: &str = "<1 ms";
    /// Reset action button.
    pub const ACTION_RESET: &str = "reset breaker";
    /// Toggle action labels — the action that will happen.
    pub const ACTION_DISABLE: &str = "disable";
    pub const ACTION_ENABLE: &str = "enable";
    /// Per-button hint while `CAUCE_ENGINES` pins the set (button disabled).
    pub const TOGGLE_PINNED: &str = "pinned by CAUCE_ENGINES";
    /// Card notice after an enable/disable write.
    pub const TOGGLE_SAVED: &str = "saved; applies after restart";
    /// Test-query form: input default, aria label, submit label.
    pub const TEST_DEFAULT: &str = "test";
    pub const TEST_ARIA: &str = "test query";
    pub const ACTION_RUN: &str = "run";
    /// Meta line above a test fragment; `{n}`/`{ms}` are substituted.
    pub const TEST_RESULTS: &str = "{n} results · {ms} ms";
    /// Error classes the test meta line can show (`{n} results` absent).
    pub const TEST_NO_RESULTS: &str = "no results";
    pub const TEST_BLOCKED: &str = "blocked";
    pub const TEST_TIMEOUT: &str = "timeout";
    pub const TEST_RATE_LIMITED: &str = "rate limited";
    pub const TEST_PARSE: &str = "parse error";
    pub const TEST_TRANSPORT: &str = "transport error";
    pub const TEST_UPSTREAM: &str = "upstream failed";
    pub const TEST_BREAKER_OPEN: &str = "breaker open";
    pub const TEST_NO_ENGINES: &str = "no engines";
    pub const TEST_UNKNOWN_ENGINES: &str = "unknown engines";
    pub const TEST_BAD_REQUEST: &str = "bad request";
    /// Inline meta when an htmx test fetch fails outside the handler;
    /// `{status}` is the HTTP status code.
    pub const TEST_FETCH_FAILED: &str = "test query failed: HTTP {status}";
    /// Inline meta for a failed non-test htmx call (reset/toggle);
    /// `{status}` is the HTTP status code.
    pub const REQUEST_FAILED: &str = "request failed: HTTP {status}";
}

/// `/answer` page copy (W4-03): the ask form, the streamed-answer shell
/// and every string the shell's inline JS interpolates (serialized as
/// `var S = {...}` — JS placeholders read `{name}` like Rust's).
pub mod answer {
    /// Search-box placeholder on the ask form.
    pub const PLACEHOLDER: &str = "Ask a question...";
    /// Ask submit label.
    pub const SUBMIT: &str = "ask";
    /// The `/search` meta-line link to this page.
    pub const ASK_LINK: &str = "ask";
    /// Status line while the answer stream is in flight.
    pub const WAITING: &str = "answering...";
    /// Status word once the terminal `done` frame lands.
    pub const COMPLETE: &str = "answered";
    /// Status word when the stream fails before `done`.
    pub const ERROR_STATUS: &str = "answer failed";
    /// `done.confidence` meta phrase; `{n}` is the 1-10 score.
    pub const CONFIDENCE: &str = "confidence {n}/10";
    /// `done.cached` meta word.
    pub const CACHED: &str = "cached";
    /// `done.ungrounded` notice — a zero-source answer is never cached
    /// (the core already enforces it) and gets this visible flag.
    pub const UNGROUNDED: &str = "this answer cites no sources — it may be ungrounded";
    /// `related_questions` section label.
    pub const RELATED: &str = "related";
    /// Sources section label (aria + heading).
    pub const SOURCES: &str = "sources";
    /// `error.retry_after_s` suffix; `{n}` is the seconds hint.
    pub const RETRY_AFTER: &str = "retry after {n}s";
    /// Inline error lead for HTTP-level and provider `error` frames.
    pub const STREAM_FAILED: &str = "answer stream failed";
    /// An SSE frame whose data did not parse.
    pub const INVALID_STREAM: &str = "unreadable answer stream";
    /// Disabled notice lead; followed by the `/settings` link.
    pub const DISABLED: &str = "AI mode is disabled.";
    /// Disabled notice link text to `/settings`.
    pub const DISABLED_LINK: &str = "configure it in settings";
    /// `<noscript>` line inside the stream shell.
    pub const NOSCRIPT: &str =
        "Answering needs JavaScript; the answer stream cannot run without it.";
}
