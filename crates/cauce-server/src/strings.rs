//! Shared English UI copy for the server pages.
//!
//! One shape across pages (`decisions.md`): `common` holds cross-page
//! literals; each page module exposes flat `pub const` items referenced from
//! templates as `crate::strings::<page>::NAME`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub mod common {
    pub const BRAND: &str = "cauce";
    pub const DASH: &str = "-";
    pub const NAV_SEARCH: &str = "search";
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
    pub const ROWS: &str = "rows";
    pub const ROWS_MATCHING: &str = "rows matching";
    pub const CAP_NOTE_PREFIX: &str = "showing the newest";
    pub const CAP_NOTE_SUFFIX: &str = "raise `limit` (max 1000) for more";
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
}
