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
    /// Link back to the search landing page.
    pub const NAV_SEARCH: &str = "search";
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

/// `/engines` page copy (W2-05).
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
}
