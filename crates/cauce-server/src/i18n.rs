//! User-visible copy for the HTMX pages.
//!
//! The old `strings.rs` consts live in the rust-i18n catalog
//! `locales/en.yaml` (embedded at compile time; `[ui].locale` picks the
//! catalog at startup — per-request `Accept-Language` is out of scope).
//! Rust call sites use `t!("module.key")`; Askama templates cannot call
//! macros inside `{{ ... }}` expressions, so they go through
//! [`crate::i18n::tr`].
//!
//! The `var S = {...}`/`var AS = {...}` literals the pages' inline JS
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

/// Look up one catalog entry (`"cache.empty"`). Askama parses
/// `{{ crate::i18n::tr("key") }}` as a function call but cannot expand
/// `t!(...)` in the same position, so every template reference goes
/// through this fn.
/// `key` is `&'static` because `_rust_i18n_translate` ties the returned
/// `Cow`'s lifetime to the key, and every call site passes a literal.
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

/// The `var S = {...}` copy the `/answer` shell's inline JS interpolates
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
    ])
}
