//! User-visible copy for the HTMX pages.
//!
//! The old `strings.rs` consts live in the rust-i18n catalog
//! `locales/en.yaml` (embedded at compile time; `[ui].locale` picks the
//! catalog at startup — per-request `Accept-Language` is out of scope).
//! Rust call sites use `t!("module.key")`; Askama templates cannot call
//! macros inside `{{ ... }}` expressions, so they go through
//! [`crate::i18n::tr`].
//!
//! The `var S`/`var SA`/`var AS` literals the pages' inline JS
//! reads are built here too ([`search_bundle`], [`assist_bundle`],
//! [`answer_bundle`]), and `cargo run -p cauce-server --bin gen_i18n`
//! writes the same maps to `web/src/i18n/*.json` — so the TypeScript
//! side typechecks against real copy and vitest asserts on it.
//!
//! Placeholders stay `{name}`-style (`{n} results`, `retry after {n}s`):
//! both the Rust `.replace()` call sites and the bundled JS do their own
//! brace substitution, so no `%{...}` interpolation is used.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::borrow::Cow;

use serde_json::{Map, Value};

/// Look up one catalog entry (`"cache.empty"`). `key` is `&'static`
/// because `_rust_i18n_translate` ties the returned `Cow`'s lifetime to
/// the key, and every call site passes a literal.
pub fn tr(key: &'static str) -> Cow<'static, str> {
    rust_i18n::t!(key)
}

/// Build a `{js_name: text}` map from catalog keys; the JS name is the
/// last path segment (`search.no_results` -> `no_results`).
fn bundle(keys: &[&'static str]) -> Value {
    let map: Map<String, Value> = keys
        .iter()
        .map(|&k| {
            (
                k.rsplit('.').next().unwrap_or(k).to_string(),
                Value::String(tr(k).into_owned()),
            )
        })
        .collect();
    Value::Object(map)
}

/// The `var S = {...}` copy the `/search` streaming page's inline JS
/// interpolates (mirrored to `web/src/i18n/search.json` by `gen_i18n`).
pub fn search_bundle() -> Value {
    bundle(&[
        "search.results",
        "search.no_results",
        "search.waiting",
        "search.complete",
        "search.invalid_stream",
        "search.new_above",
        "search.live_badge",
        "search.cached",
        // Full cache badge (`cached · {age} s ago · ttl {ttl} s`): SSR-only
        // on the HTMX page, but the SPA's JSON (non-stream) path renders
        // the same meta line client-side (FX-03).
        "search.cached_badge",
        "search.stale_badge",
        "search.engine_failed",
        "search.engine_skipped",
        "search.err_rate_limited",
        "search.err_blocked",
        "search.err_timeout",
        "search.err_parse",
        "search.err_transport",
        "search.err_no_results",
        "search.err_unknown",
    ])
}

/// The `var AS = {...}` copy the assist card's inline JS interpolates
/// (mirrored to `web/src/i18n/assist.json`).
pub fn assist_bundle() -> Value {
    bundle(&[
        "assist.stream_failed",
        "assist.invalid_stream",
        "assist.retry_after",
        // W7-03: grounded/confidence chips in the card meta row.
        "assist.grounded",
        "assist.ungrounded",
        "assist.confidence",
        "assist.cached",
    ])
}

/// The `/app` SPA's chrome + form copy (FX-03), nested by module so the
/// TypeScript side reads `spa.common.nav_search`, `spa.assist.trigger`,
/// ... (mirrored to `web/src/i18n/spa.json` by `gen_i18n`). Stream-time
/// copy stays in the flat `search`/`assist`/`answer` bundles — the SPA
/// imports those for the same keys the inline `var S`/`var AS`/`var SA`
/// literals carried.
pub fn spa_bundle() -> Value {
    let mut root = Map::new();
    for (module, keys) in [
        (
            "common",
            &[
                "common.brand",
                "common.dash",
                "common.nav_search",
                "common.nav_answer",
                "common.nav_history",
                "common.nav_dashboard",
                "common.nav_archive",
                "common.nav_engines",
                "common.nav_cache",
                "common.nav_audit",
                "common.nav_settings",
                "common.nav_more",
                "common.nav_primary_label",
                "common.nav_operator_label",
                "common.theme_switch",
                "common.theme_system",
                "common.theme_light",
                "common.theme_dark",
                "common.theme_aria",
                "common.theme_aria_state",
                "common.request_label",
            ][..],
        ),
        (
            "search",
            &[
                "search.placeholder",
                "search.submit",
                "search.ai_mode",
                "search.more",
                "search.searching",
            ][..],
        ),
        (
            "assist",
            &[
                "assist.trigger",
                "assist.label",
                "assist.ask_ai",
                "assist.disclaimer",
                "assist.noscript",
            ][..],
        ),
        (
            "answer",
            &["answer.placeholder", "answer.submit", "answer.ask_link"][..],
        ),
        (
            "app",
            &[
                "app.not_found",
                "app.open_html",
                "app.stream_hint",
                // FX-07 instance modes: gate notice + token form,
                // public dashboard card, archive-off line, cache
                // details expander.
                "app.admin_gate_title",
                "app.admin_gate_note",
                "app.admin_token_label",
                "app.admin_token_hint",
                "app.admin_token_save",
                "app.admin_token_clear",
                "app.instance_card",
                "app.instance_version",
                "app.instance_engines",
                "app.instance_privacy",
                "app.archiving_off",
                "app.cache_details",
            ][..],
        ),
        // FX-05 admin/read pages — the whole `history`/`dashboard`/
        // `settings`/`engines`/`cache`/`audit`/`archive` catalogs so the
        // SPA keeps the HTMX pages' strings verbatim.
        (
            "history",
            &[
                "history.title",
                "history.stat_searches_24h",
                "history.stat_total",
                "history.stat_clicks_today",
                "history.filter_submit",
                "history.window_24h",
                "history.window_7d",
                "history.window_30d",
                "history.window_all",
                "history.since_label",
                "history.origin_label",
                "history.origin_mine",
                "history.origin_agent",
                "history.origin_all",
                "history.query_label",
                "history.query_placeholder",
                "history.cached_only",
                "history.clear",
                "history.empty",
                "history.ef_match",
                "history.ef_none",
                "history.in_24h",
                "history.in_7d",
                "history.in_30d",
                "history.in_since",
                "history.ef_cached",
                "history.ef_origin",
                "history.ef_origin_all",
                "history.capped_showing",
                "history.capped_of",
                "history.capped_hint",
                "history.col_query",
                "history.col_when",
                "history.col_source",
                "history.col_engines",
                "history.col_results",
                "history.col_latency",
                "history.col_client",
                "history.src_cached",
                "history.src_expired",
                "history.src_network",
                "history.tier_prefix",
                "history.position_prefix",
                "history.clicks_word",
                "history.click_one",
                "history.click_only",
                "history.rerun",
                "history.rerun_answer",
                "history.copy_json",
                "history.copied",
                "history.payload",
                "history.delete",
                "history.delete_confirm",
                "history.delete_confirm_answer",
                "history.chip_agent",
                "history.chip_ai",
                "history.delete_confirm_clicks_pre",
                "history.delete_failed",
                "history.request_label",
            ][..],
        ),
        (
            "dashboard",
            &[
                "dashboard.title",
                "dashboard.window",
                "dashboard.days_7",
                "dashboard.days_30",
                "dashboard.no_data",
                "dashboard.no_engines",
                "dashboard.searches_per_day",
                "dashboard.legend_cache",
                "dashboard.legend_network",
                "dashboard.hit_rate",
                "dashboard.latency",
                "dashboard.ttfr",
                "dashboard.full",
                "dashboard.clients",
                "dashboard.outcomes",
                "dashboard.top_queries",
                "dashboard.zero_results",
                "dashboard.reliability",
                "dashboard.deadline_hits",
                "dashboard.stale_served",
                "dashboard.admission_rejected",
                "dashboard.engines",
                "dashboard.cache",
                "dashboard.engine_eval",
                "dashboard.eval_no_run",
                "dashboard.col_engine",
                "dashboard.col_breaker",
                "dashboard.col_reliability",
                "dashboard.col_calls",
                "dashboard.col_total",
                "dashboard.col_http",
                "dashboard.col_parse",
                "dashboard.cache_rows",
                "dashboard.cache_unexpired",
                "dashboard.cache_db_size",
                "dashboard.cache_newest",
            ][..],
        ),
        (
            "settings",
            &[
                "settings.page_title",
                "settings.editing",
                "settings.restart_note",
                "settings.section_search",
                "settings.deadline",
                "settings.ttl",
                "settings.min_results",
                "settings.hedge_floor",
                "settings.hedge_ceiling",
                "settings.section_engines",
                "settings.engines_pinned",
                "settings.enabled",
                "settings.disabled",
                "settings.tier",
                "settings.tier_default",
                "settings.proxy",
                "settings.proxy_placeholder",
                "settings.browse_engines",
                "settings.section_admission",
                "settings.max_wait",
                "settings.max_concurrent",
                "settings.section_logging",
                "settings.retention",
                "settings.report_link",
                "settings.report_hint",
                "settings.section_cache",
                "settings.entries",
                "settings.unexpired",
                "settings.newest",
                "settings.delete_expired",
                "settings.delete_all",
                "settings.confirm_expired",
                "settings.confirm_all",
                "settings.browse_entries",
                "settings.section_ai",
                "settings.ai_base_url",
                "settings.ai_api_key",
                "settings.ai_api_key_placeholder",
                "settings.ai_model",
                "settings.ai_model_placeholder",
                "settings.ai_max_turns",
                "settings.ai_max_searches",
                "settings.ai_provider_budget",
                "settings.models_unreachable",
                "settings.applies_after_restart",
                "settings.restart_needed",
                "settings.set_by",
                "settings.is_set",
                "settings.is_not_set",
                "settings.save",
                "settings.saved",
                "settings.invalid_number",
                "settings.not_saved",
                "settings.error_one",
                "settings.error_many",
                "settings.could_not_save",
                "settings.noscript",
                "settings.section_byok",
                "settings.byok_hint",
                "settings.byok_protocol",
                "settings.byok_protocol_default",
                "settings.byok_base_url_locked",
            ][..],
        ),
        (
            "engines",
            &[
                "engines.page_title",
                "engines.summary",
                "engines.summary_open",
                "engines.hint_restart",
                "engines.hint_pinned",
                "engines.empty",
                "engines.not_in_config",
                "engines.not_running",
                "engines.breaker_closed",
                "engines.breaker_open",
                "engines.breaker_half_open",
                "engines.breaker_retries",
                "engines.breaker_elapsed",
                "engines.breaker_probing",
                "engines.disabled",
                "engines.stat_enabled",
                "engines.stat_ewma",
                "engines.stat_last_ok",
                "engines.stat_last_error",
                "engines.stat_p95",
                "engines.stat_reliability",
                "engines.stat_requests",
                "engines.enabled_yes",
                "engines.enabled_no",
                "engines.last_ok_fmt",
                "engines.sub_ms",
                "engines.action_reset",
                "engines.action_disable",
                "engines.action_enable",
                "engines.toggle_pinned",
                "engines.toggle_saved",
                "engines.toggle_saved_restart",
                "engines.test_default",
                "engines.test_aria",
                "engines.action_run",
                "engines.test_results",
                "engines.test_no_results",
                "engines.test_blocked",
                "engines.test_timeout",
                "engines.test_rate_limited",
                "engines.test_parse",
                "engines.test_transport",
                "engines.test_upstream",
                "engines.test_breaker_open",
                "engines.test_no_engines",
                "engines.test_unknown_engines",
                "engines.test_bad_request",
                "engines.test_fetch_failed",
                "engines.request_failed",
            ][..],
        ),
        (
            "cache",
            &[
                "cache.page_title",
                "cache.filter_placeholder",
                "cache.filter_button",
                "cache.filter_clear",
                "cache.delete_expired",
                "cache.delete_all",
                "cache.confirm_expired",
                "cache.confirm_all",
                "cache.confirm_row",
                "cache.delete_row",
                "cache.empty",
                "cache.empty_filtered",
                "cache.page_prev",
                "cache.page_next",
                "cache.filtered_cap",
                "cache.payload_loading",
                "cache.payload_error",
                "cache.entry_one",
                "cache.entry_many",
                "cache.matching",
                "cache.hit_one",
                "cache.hit_many",
                "cache.expires_in",
                "cache.expired_ago",
            ][..],
        ),
        (
            "audit",
            &[
                "audit.page_title",
                "audit.any",
                "audit.filter",
                "audit.clear",
                "audit.empty",
                "audit.filtered_empty_prefix",
                "audit.filtered_empty_and",
                "audit.actor_label",
                "audit.action_label",
                "audit.when_column",
                "audit.actor_column",
                "audit.action_column",
                "audit.target_column",
                "audit.request_column",
                "audit.details_summary",
                "audit.row",
                "audit.rows",
                "audit.matching",
                "audit.cap_note_prefix",
                "audit.cap_note_suffix",
                "audit.table_region",
            ][..],
        ),
        (
            "archive",
            &[
                "archive.page_title",
                "archive.filter_placeholder",
                "archive.filter_button",
                "archive.filter_clear",
                "archive.confirm_row",
                "archive.delete_row",
                "archive.empty",
                "archive.empty_filtered",
                "archive.disabled",
                "archive.disabled_link",
                "archive.page_prev",
                "archive.page_next",
                "archive.markdown_loading",
                "archive.markdown_failed",
                "archive.markdown_error",
                "archive.page_one",
                "archive.page_many",
                "archive.matching",
            ][..],
        ),
        (
            "admin",
            &[
                "admin.title",
                "admin.tabs_label",
                "admin.tab_engines",
                "admin.tab_cache",
                "admin.tab_audit",
            ][..],
        ),
    ] {
        root.insert(module.to_string(), bundle(keys));
    }
    Value::Object(root)
}

/// The `var SA = {...}` copy the `/answer` shell's inline JS interpolates
/// (mirrored to `web/src/i18n/answer.json`).
pub fn answer_bundle() -> Value {
    bundle(&[
        "answer.waiting",
        "answer.complete",
        "answer.error_status",
        "answer.confidence",
        "answer.cached",
        "answer.ungrounded",
        // W7-03: retrieval-path chip + tool display words.
        "answer.path_direct",
        "answer.path_searched",
        "answer.path_replay",
        "answer.tool_web",
        "answer.tool_archive",
        "answer.related",
        "answer.sources",
        "answer.retry_after",
        "answer.stream_failed",
        "answer.invalid_stream",
        // W7-04: the pinned follow-up form's copy (SSR'd via tr() too,
        // kept in the bundle so the JS side can reuse it).
        "answer.followup_placeholder",
        "answer.followup_submit",
        // FX-04 SPA: stop/edit affordances + the collapsed-steps label;
        // disabled notice + the short ungrounded badge word (the HTMX
        // page SSRs them via tr(), the SPA reads them from `SA`).
        "answer.stop",
        "answer.stopped",
        "answer.steps",
        "answer.edit",
        "answer.ask_prompt",
        "answer.ungrounded_badge",
        "answer.disabled",
        "answer.disabled_link",
        "answer.placeholder",
        "answer.submit",
    ])
}
