//! Shared English UI copy for the audit and trace pages.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub(crate) struct AuditStrings {
    pub(crate) title: &'static str,
    pub(crate) heading: &'static str,
    pub(crate) actor_placeholder: &'static str,
    pub(crate) action_placeholder: &'static str,
    pub(crate) filter: &'static str,
    pub(crate) clear: &'static str,
    pub(crate) filtered_empty: &'static str,
    pub(crate) empty: &'static str,
    pub(crate) when_column: &'static str,
    pub(crate) actor_column: &'static str,
    pub(crate) action_column: &'static str,
    pub(crate) target_column: &'static str,
    pub(crate) request_column: &'static str,
    pub(crate) details_column: &'static str,
    pub(crate) details_summary: &'static str,
    pub(crate) missing_request: &'static str,
    pub(crate) rows_label: &'static str,
}

pub(crate) const AUDIT: AuditStrings = AuditStrings {
    title: "audit · cauce",
    heading: "Audit",
    actor_placeholder: "actor (ui, api, cli, mcp:...)",
    action_placeholder: "action (cache.delete, ...)",
    filter: "Filter",
    clear: "clear",
    filtered_empty: "No audit rows match these filters.",
    empty: "No audit rows yet.",
    when_column: "when",
    actor_column: "actor",
    action_column: "action",
    target_column: "target",
    request_column: "request",
    details_column: "details",
    details_summary: "details",
    missing_request: "-",
    rows_label: "rows",
};

pub(crate) struct TraceStrings {
    pub(crate) title_prefix: &'static str,
    pub(crate) title_suffix: &'static str,
    pub(crate) heading: &'static str,
}

pub(crate) const TRACE: TraceStrings = TraceStrings {
    title_prefix: "trace ",
    title_suffix: " · cauce",
    heading: "Trace",
};
