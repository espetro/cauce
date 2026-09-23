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

pub mod settings {
    pub const PAGE_TITLE: &str = "Settings";
    pub const EDITING: &str = "editing";
    pub const RESTART_NOTE: &str = "changes apply on restart";

    pub const SECTION_SEARCH: &str = "Search";
    pub const DEADLINE: &str = "Deadline (ms)";
    pub const TTL: &str = "Cache TTL (s)";
    pub const HEDGE: &str = "Hedge threshold (ms)";
    pub const HEDGE_WAVE: &str = "lands in wave 3";

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
    pub const AI_WAVE: &str = "lands in wave 4";
    pub const AI_BASE_URL: &str = "Base URL";
    pub const AI_API_KEY: &str = "API key";
    pub const AI_API_KEY_PLACEHOLDER: &str = "${env:BIFROST_API_KEY}";
    pub const AI_MODEL: &str = "Model";
    pub const AI_MODEL_PLACEHOLDER: &str = "model name";
    pub const MODELS_UNREACHABLE: &str = "model list unreachable; type a model name";
    pub const AI_PIPELINE_NOTE: &str = "(the answer pipeline arrives in wave 4)";

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
    /// Every user-visible string on the history page.
    #[derive(Debug, Clone, Copy)]
    pub struct Copy {
        pub nav_search: &'static str,
        pub nav_history: &'static str,
        pub heading: &'static str,
        /// Stats line: `N searches in last 24h · N total · N clicks today`.
        pub stat_searches_24h: &'static str,
        pub stat_total: &'static str,
        pub stat_clicks_today: &'static str,
        pub filter_submit: &'static str,
        pub window_24h: &'static str,
        pub window_7d: &'static str,
        pub window_30d: &'static str,
        pub window_all: &'static str,
        pub query_placeholder: &'static str,
        pub cached_only: &'static str,
        pub clear: &'static str,
        pub empty: &'static str,
        /// Filtered-empty fragments, composed as
        /// `no searches match "q" in the last 7 days that are still cached.`
        pub ef_match: &'static str,
        pub ef_none: &'static str,
        pub in_24h: &'static str,
        pub in_7d: &'static str,
        pub in_30d: &'static str,
        pub in_since: &'static str,
        pub ef_cached: &'static str,
        /// Cap note: `showing 200 of N · use the filters to reach older searches`.
        pub capped_showing: &'static str,
        pub capped_of: &'static str,
        pub capped_hint: &'static str,
        pub col_query: &'static str,
        pub col_when: &'static str,
        pub col_source: &'static str,
        pub col_engines: &'static str,
        pub col_results: &'static str,
        pub col_latency: &'static str,
        pub col_client: &'static str,
        /// Source-cell fragments: `cached · 41m`, `cached · expired`,
        /// `network · t1`.
        pub src_cached: &'static str,
        pub src_expired: &'static str,
        pub src_network: &'static str,
        pub clicks_word: &'static str,
        pub click_only: &'static str,
        pub rerun: &'static str,
        pub copy_json: &'static str,
        pub copied: &'static str,
        pub payload: &'static str,
        pub delete: &'static str,
        pub delete_confirm: &'static str,
        pub request_label: &'static str,
    }

    /// The English copy (only language in v3.0).
    pub const COPY: Copy = Copy {
        nav_search: "search",
        nav_history: "history",
        heading: "History",
        stat_searches_24h: "searches in last 24h",
        stat_total: "total",
        stat_clicks_today: "clicks today",
        filter_submit: "Filter",
        window_24h: "last 24h",
        window_7d: "last 7d",
        window_30d: "last 30d",
        window_all: "all time",
        query_placeholder: "filter query text...",
        cached_only: "cached only",
        clear: "clear",
        empty: "nothing searched yet. run a search and it lands here.",
        ef_match: "no searches match",
        ef_none: "no searches",
        in_24h: "in the last 24h",
        in_7d: "in the last 7 days",
        in_30d: "in the last 30 days",
        in_since: "since",
        ef_cached: "that are still cached",
        capped_showing: "showing",
        capped_of: "of",
        capped_hint: "use the filters to reach older searches",
        col_query: "query",
        col_when: "when",
        col_source: "source",
        col_engines: "engines",
        col_results: "n",
        col_latency: "ms",
        col_client: "by",
        src_cached: "cached",
        src_expired: "expired",
        src_network: "network",
        clicks_word: "clicks",
        click_only: "(click only)",
        rerun: "re-run",
        copy_json: "copy json",
        copied: "copied",
        payload: "payload",
        delete: "delete",
        delete_confirm: "Delete this search?",
        request_label: "request",
    };
}
