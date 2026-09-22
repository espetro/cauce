//! User-visible copy for the HTMX pages.
//!
//! The wave-2 settled inputs require all page strings to live here so a
//! later i18n pass is a mechanical change, not a rewrite. Templates
//! reference these as `crate::strings::*` path expressions.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// `/cache` page (W2-04).
pub const CACHE_PAGE_TITLE: &str = "Cache";
/// Search-box placeholder on `/cache`.
pub const CACHE_FILTER_PLACEHOLDER: &str = "Filter cached queries...";
/// Search-box submit label on `/cache`.
pub const CACHE_FILTER_BUTTON: &str = "Filter";
/// "Clear filter" link shown while `q` is active.
pub const CACHE_FILTER_CLEAR: &str = "clear";
/// Bulk action: delete rows past `expires_at`.
pub const CACHE_DELETE_EXPIRED: &str = "Delete expired";
/// Bulk action: delete every row.
pub const CACHE_DELETE_ALL: &str = "Delete all";
/// `hx-confirm` for the expired-rows bulk delete.
pub const CACHE_CONFIRM_EXPIRED: &str = "Delete every expired cache entry?";
/// `hx-confirm` for the full-table delete.
pub const CACHE_CONFIRM_ALL: &str =
    "Delete ALL cache entries? Every next search will hit the network.";
/// `hx-confirm` for a single-row delete.
pub const CACHE_CONFIRM_ROW: &str = "Delete this cache entry?";
/// Per-row delete button label.
pub const CACHE_DELETE_ROW: &str = "Delete";
/// Empty state when the table has no rows at all.
pub const CACHE_EMPTY: &str = "The cache is empty. Run a search first.";
/// Empty state when `q` matched nothing.
pub const CACHE_EMPTY_FILTERED: &str = "No cache entries match this filter.";
/// Pagination: link to the previous (newer) page.
pub const CACHE_PAGE_PREV: &str = "Newer";
/// Pagination: link to the next (older) page.
pub const CACHE_PAGE_NEXT: &str = "Older";
/// Note shown in filtered mode, which has no pagination.
pub const CACHE_FILTERED_CAP_NOTE: &str = "top matches by rank";
/// Placeholder inside the payload slot before the lazy fragment loads.
pub const CACHE_PAYLOAD_LOADING: &str = "loading...";
/// Link back to the search landing page.
pub const NAV_SEARCH: &str = "search";
