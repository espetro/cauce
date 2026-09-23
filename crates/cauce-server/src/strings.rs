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
