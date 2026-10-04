//! The `safe`/`verbose` redaction profiles (#241, from the #235 design
//! comment).
//!
//! `safe` (the default) is the design's list, applied over the assembled
//! bundle:
//!
//! - `config` only ever comes from `display_tree()` — this pass re-asserts
//!   the secret-leaf contract at the JSON layer (`ai.api_key` and every
//!   `engines.<i>.env.*` non-empty, non-`${...}`, non-`<redacted>` string
//!   becomes `<redacted>`), so a hand-built or future section cannot leak
//!   one either.
//! - Query text folds into its `CacheKey` hash: `query`/`query_raw`
//!   leaves become a `query_hash` — `CacheKey` of `normalize_query`, the
//!   same value the `search_log` row stored for the request, so hashed
//!   rows still join — unless a sibling `query_hash` already exists;
//!   `zero_result_queries` strings hash in place.
//! - `fields`/`params`/`params_json`/`details`/`details_json` subtrees
//!   lose every `query|q|prompt|messages|content` leaf outright (span
//!   `fields.query`, audit `details.query`, ...); the `q|prompt|messages|
//!   content` leaf names are stripped anywhere else they appear bare.
//! - URL credentials (userinfo) are stripped everywhere, and under `safe`
//!   the URL's own `?` component goes too — an upstream search URL
//!   encodes the query text (`https://h/search?q=…` is a `q` leaf in
//!   disguise). URLs are scrubbed whether a leaf is a bare URL or embeds
//!   one in error text.
//! - `audit_tail[].actor` keeps client-kind labels (`ui | api | cli |
//!   mcp:<name>`) and drops free-form `X-Actor` overrides.
//!
//! `verbose` (`--include-queries`) keeps raw query text — the owner's own
//!   debugging sessions — but never secrets or URL credentials. The
//!   profile is a per-export flag, never persisted.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use url::Url;

use super::ReportBundle;
use crate::cache::{normalize_query, push_str};

/// What the export's redaction profile means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionProfile {
    /// The default: no query text, no secrets, no URL credentials.
    #[default]
    Safe,
    /// `--include-queries`: raw query text survives, for the owner's own
    /// debugging sessions. Secrets and URL userinfo still never do.
    Verbose,
}

impl RedactionProfile {
    /// The serde spelling (`"safe"`/`"verbose"`) — what the bundle's
    /// `profile` field serializes as.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Verbose => "verbose",
        }
    }
}

/// `display_tree`'s marker, reused for leaves this pass redacts.
const REDACTED: &str = "<redacted>";

/// Leaves stripped anywhere they appear outside a scrub zone —
/// `q|prompt|messages|content`. `query`/`query_raw` get the hash fold
/// instead (see [`fold_query_leaves`]).
const STRIP_LEAVES: &[&str] = &["q", "prompt", "messages", "content"];
/// Inside a scrub zone, `query` is stripped rather than hashed — span
/// `fields.query` has no `query_hash` sibling to join to.
const SCRUB_ZONE_LEAVES: &[&str] = &["query", "q", "prompt", "messages", "content"];
/// Object names whose whole subtree gets the leaf sweep: tracing span
/// `fields`, exec `params_json`, audit `details_json` and friends.
const SCRUB_OBJECTS: &[&str] = &["fields", "params", "params_json", "details", "details_json"];
/// Column-shaped query text — hashed so row joins survive.
const HASH_LEAVES: &[&str] = &["query", "query_raw"];

/// `CacheKey`-shaped hash of `normalize_query(q)` — the same 64-hex
/// sha256 digest `CacheKey` produces, over the same length-framed
/// preimage (`push_str`), so a hashed query reads like any other
/// `query_hash` in the bundle.
fn hash_query(q: &str) -> String {
    let mut buf = Vec::with_capacity(32);
    push_str(&mut buf, &normalize_query(q));
    format!("{:x}", Sha256::digest(&buf))
}

/// Whether `actor` is a client-kind label (`ui | api | cli | mcp:<name>`)
/// rather than a free-form `X-Actor` override.
fn is_client_label(actor: &str) -> bool {
    matches!(actor, "ui" | "api" | "cli")
        || actor.strip_prefix("mcp:").is_some_and(|n| !n.is_empty())
}

/// Run `bundle` through its profile's rules. Called by
/// `ReportBundle::collect`; `Verbose` skips the query rules only —
/// secrets, URL userinfo and actor normalization apply in both profiles.
pub(super) fn apply(bundle: &mut ReportBundle) {
    let strip_queries = bundle.profile == RedactionProfile::Safe;
    for (name, section) in &mut bundle.sections {
        match name.as_str() {
            "config" => scrub_secret_paths(section),
            "audit_tail" => scrub_actors(section),
            _ => {}
        }
        walk(section, false, strip_queries);
    }
}

/// Recursive sweep over one section payload.
///
/// - Inside a [`SCRUB_OBJECTS`] subtree (`in_scrub`), every
///   [`SCRUB_ZONE_LEAVES`] leaf is removed outright (query text there is
///   span fields or free-form params — nothing to join to).
/// - Outside one, `query`/`query_raw` string leaves fold into a
///   `query_hash` (dropped when a sibling `query_hash` already carries
///   the join), `zero_result_queries` string arrays hash in place, and
///   [`STRIP_LEAVES`] are removed.
/// - Every string leaf anywhere gets [`scrub_urls_in_text`].
fn walk(v: &mut Value, in_scrub: bool, strip_queries: bool) {
    match v {
        Value::Object(map) => {
            if !in_scrub && strip_queries {
                fold_query_leaves(map);
            }
            let doomed: Vec<String> = map
                .keys()
                .filter(|k| {
                    strip_queries
                        && if in_scrub {
                            SCRUB_ZONE_LEAVES.contains(&k.as_str())
                        } else {
                            STRIP_LEAVES.contains(&k.as_str())
                        }
                })
                .cloned()
                .collect();
            for k in doomed {
                map.remove(&k);
            }
            for (k, child) in map.iter_mut() {
                // `zero_result_queries` is an array of bare query strings —
                // hash each element; arrays otherwise recurse untouched.
                if strip_queries && k == "zero_result_queries" {
                    if let Value::Array(items) = child {
                        for item in items.iter_mut() {
                            if let Value::String(s) = item {
                                *s = hash_query(s);
                            }
                        }
                    }
                    continue;
                }
                walk(
                    child,
                    in_scrub || SCRUB_OBJECTS.contains(&k.as_str()),
                    strip_queries,
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, in_scrub, strip_queries);
            }
        }
        Value::String(s) => scrub_urls_in_text(s, strip_queries),
        _ => {}
    }
}

/// Fold `query`/`query_raw` string leaves into `query_hash`: when a
/// sibling `query_hash` already exists (a `search_log` row), the leaves
/// are just dropped — the join is preserved; otherwise the normalized
/// text's `CacheKey` stands in (`top_queries` entries, `eval` outcomes,
/// ...).
fn fold_query_leaves(map: &mut Map<String, Value>) {
    let mut text: Option<String> = None;
    for key in HASH_LEAVES {
        if let Some(Value::String(s)) = map.get(*key) {
            text.get_or_insert_with(|| s.clone());
        }
    }
    if let Some(q) = text {
        map.remove("query");
        map.remove("query_raw");
        map.entry("query_hash".to_string())
            .or_insert_with(|| Value::String(hash_query(&q)));
    }
}

/// Re-apply `display_tree`'s secret-leaf contract at the JSON layer —
/// `ai.api_key` and every `engines.<i>.env.*` non-empty, non-`${...}`,
/// non-`<redacted>` string becomes `<redacted>`. Belt over the settled
/// rule that `config` only ever comes from `display_tree()`.
fn scrub_secret_paths(config: &mut Value) {
    if let Some(v) = config.pointer_mut("/ai/api_key") {
        redact_leaf(v);
    }
    if let Some(engines) = config.get_mut("engines").and_then(Value::as_array_mut) {
        for entry in engines {
            if let Some(env) = entry.get_mut("env").and_then(Value::as_object_mut) {
                for value in env.values_mut() {
                    redact_leaf(value);
                }
            }
        }
    }
}

/// A non-empty, non-template, non-already-redacted string leaf at a
/// secret path becomes `<redacted>` (mirrors `config::redact`'s rules:
/// `${...}` templates print as their raw text, `""` stays `""`).
fn redact_leaf(v: &mut Value) {
    let needs = matches!(
        v,
        Value::String(s) if !s.is_empty() && s != REDACTED && !s.starts_with("${")
    );
    if needs {
        *v = Value::String(REDACTED.to_string());
    }
}

/// `audit_tail[].actor`: keep client-kind labels, drop free-form
/// `X-Actor` overrides (a name or email someone typed is personal data).
fn scrub_actors(audit_tail: &mut Value) {
    let Some(rows) = audit_tail.as_array_mut() else {
        return;
    };
    for row in rows {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        let needs = matches!(
            obj.get("actor"),
            Some(Value::String(a)) if !is_client_label(a)
        );
        if needs {
            obj.insert("actor".to_string(), Value::String(REDACTED.to_string()));
        }
    }
}

/// Scrub URLs inside `s`: each `scheme://…` token is parsed and rebuilt
/// without userinfo — and, under `safe`, without its `?` component. The
/// query string goes because upstream search URLs embed the query text in
/// `?q=`-style params — a URL query leaf is a query leaf. URLs embedded
/// in prose (`"dial https://u:p@h/x?q=…"`) are scrubbed too, not just
/// bare-URL leaves. Tokens that don't parse (or that would serialize
/// unchanged) are emitted verbatim so non-URL text is byte-stable.
fn scrub_urls_in_text(s: &mut String, strip_query: bool) {
    const DELIMS: &[char] = &[
        ' ', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>', '\t', '\n',
    ];
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find("://") {
        let before = &rest[..i];
        // Scheme chars immediately before `://`; must start with a letter.
        let scheme_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'))
            .map_or(0, |p| p + 1);
        let scheme = &before[scheme_start..];
        if !scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            out.push_str(&rest[..i + 3]);
            rest = &rest[i + 3..];
            continue;
        }
        let token_end = rest[i + 3..]
            .find(|c: char| DELIMS.contains(&c))
            .map_or(rest.len(), |e| i + 3 + e);
        let candidate = &rest[scheme_start..token_end];
        let rewritten = Url::parse(candidate).ok().and_then(|mut url| {
            let has_userinfo = !url.username().is_empty() || url.password().is_some();
            let has_query = url.query().is_some();
            (has_userinfo || (strip_query && has_query)).then(|| {
                let _ = url.set_username("");
                let _ = url.set_password(None);
                if strip_query {
                    url.set_query(None);
                }
                url.to_string()
            })
        });
        match rewritten {
            Some(new) => {
                out.push_str(&rest[..scheme_start]);
                out.push_str(&new);
            }
            None => out.push_str(&rest[..token_end]),
        }
        rest = &rest[token_end..];
    }
    out.push_str(rest);
    *s = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{ReportBundle, SCHEMA_VERSION};
    use chrono::Utc;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::str::FromStr;

    /// A bundle deliberately seeded with every leak shape the `safe`
    /// profile is required to kill.
    fn dirty_bundle(profile: RedactionProfile) -> ReportBundle {
        let mut sections = BTreeMap::new();
        sections.insert(
            "config".into(),
            json!({
                "ai": {"api_key": "sk-live-secret", "model": "claude-x"},
                "engines": [
                    {"id": "x", "env": {"MY_KEY": "engine-secret", "EMPTY": ""},
                     "egress": {"proxy": "http://user:pass@proxy.internal:3128"}},
                    {"id": "y", "env": {"TEMPLATED": "${env:T}"}},
                ],
            }),
        );
        sections.insert(
            "stats".into(),
            json!({
                "searches": 9,
                "top_queries": [{"query": "My Private Query", "searches": 3}],
                "zero_result_queries": ["what is my ssn"],
                "search_log": [{"query_hash": "0123abcd4567ef89",
                                "query": "my private query", "clicks": 1}],
            }),
        );
        sections.insert(
            "audit_tail".into(),
            json!([
                {"actor": "Jane Doe <jane@example.com>", "action": "history.delete",
                 "target": "42", "details": {"query": "my private query", "clicks_removed": 2}},
                {"actor": "mcp:claude", "action": "mcp.search_web",
                 "target": "cache", "details": {"prompt": "ignore previous"}},
                {"actor": "api", "action": "config.put", "target": "config",
                 "details": {"paths": ["search.ttl_s"]}},
            ]),
        );
        sections.insert(
            "errors_tail".into(),
            json!([
                {"kind": "event", "level": "ERROR", "target": "cauce::pipeline",
                 "fields": {"message": "engine failed", "query": "my private query",
                            "url": "https://u:p@bing.example/search?q=my+private+query",
                            "error": "transport https://a:b@up.example/?q=my+private+query failed"}},
                {"kind": "span_close", "level": "WARN", "target": "x",
                 "span": {"id": 1, "name": "engine_http",
                          "fields": {"query": "my private query",
                                     "url": "https://bing.example/search?q=my+private+query"}},
                 "fields": {"nested": {"content": "prompt text"}}},
            ]),
        );
        sections.insert(
            "engines".into(),
            json!([{"engine": "bing", "last_error": "dial https://u:p@h/x?q=abc"},
                   {"engine": "ddgs", "last_error": "timeout"}]),
        );
        sections.insert(
            "eval_latest".into(),
            json!({"outcomes": [{"engine": "bing", "query": "eval q", "hit": true}]}),
        );
        ReportBundle {
            v: SCHEMA_VERSION,
            profile,
            generated_at: Utc::now(),
            notes: String::new(),
            sections,
        }
    }

    #[test]
    fn safe_profile_leaves_no_query_or_secret_leaf() {
        let mut bundle = dirty_bundle(RedactionProfile::Safe);
        apply(&mut bundle);
        let out = serde_json::to_string_pretty(&bundle).unwrap();

        // Secrets and credentials are gone.
        for leaked in [
            "sk-live-secret",
            "engine-secret",
            "user:pass",
            "a:b@up.example",
            "u:p@bing.example",
            "u:p@h",
        ] {
            assert!(!out.contains(leaked), "{leaked:?} leaked:\n{out}");
        }
        // Query text is gone — from stats, audit details, span fields,
        // URL query components (bare or text-embedded) and eval outcomes
        // alike.
        for leaked in [
            "My Private Query",
            "my private query",
            "my+private+query",
            "what is my ssn",
            "prompt text",
            "ignore previous",
            "eval q",
            "q=abc",
        ] {
            assert!(!out.contains(leaked), "{leaked:?} leaked:\n{out}");
        }
        // Hashes stand in: `top_queries[].query` folded to a 64-hex
        // `query_hash` (CacheKey shape), `zero_result_queries` hashed in
        // place, a `search_log` row's existing `query_hash` untouched
        // while its `query` drops.
        let top = &bundle.sections["stats"]["top_queries"][0];
        let hash = top["query_hash"].as_str().expect("query_hash");
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(top.get("query").is_none());
        let zero = bundle.sections["stats"]["zero_result_queries"][0]
            .as_str()
            .expect("hash string");
        assert_eq!(zero.len(), 64);
        let log = &bundle.sections["stats"]["search_log"][0];
        assert_eq!(log["query_hash"], json!("0123abcd4567ef89"));
        assert!(log.get("query").is_none());
        assert_eq!(log["clicks"], json!(1));
        // The eval outcome's `query` folded the same way.
        let outcome = &bundle.sections["eval_latest"]["outcomes"][0];
        assert_eq!(outcome["query_hash"].as_str().unwrap().len(), 64);
        // Actors: the free-form override is dropped; client labels stay.
        let actors: Vec<&str> = bundle.sections["audit_tail"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["actor"].as_str().unwrap())
            .collect();
        assert_eq!(actors, [REDACTED, "mcp:claude", "api"]);
        // `fields`/`details` subtrees shed their query leaves but keep
        // the rest of the record.
        let audit = &bundle.sections["audit_tail"][0];
        assert_eq!(audit["details"]["clicks_removed"], json!(2));
        let span = &bundle.sections["errors_tail"][1]["span"];
        assert!(span["fields"].get("query").is_none());
        assert_eq!(span["fields"]["url"], json!("https://bing.example/search"));
        // URL userinfo gone, URL shape kept; URLs embedded in error text
        // scrubbed the same way.
        let proxy = &bundle.sections["config"]["engines"][0]["egress"]["proxy"];
        assert_eq!(proxy, &json!("http://proxy.internal:3128/"));
        assert_eq!(
            bundle.sections["errors_tail"][0]["fields"]["error"],
            json!("transport https://up.example/ failed")
        );
        assert_eq!(
            bundle.sections["engines"][0]["last_error"],
            json!("dial https://h/x")
        );
        // The non-secret fields survive untouched.
        assert_eq!(bundle.sections["config"]["ai"]["model"], json!("claude-x"));
        assert_eq!(
            bundle.sections["config"]["engines"][0]["env"]["EMPTY"],
            json!("")
        );
        assert_eq!(
            bundle.sections["config"]["engines"][1]["env"]["TEMPLATED"],
            json!("${env:T}")
        );
        assert_eq!(bundle.sections["stats"]["searches"], json!(9));
    }

    #[test]
    fn verbose_profile_keeps_queries_but_not_secrets() {
        let mut bundle = dirty_bundle(RedactionProfile::Verbose);
        apply(&mut bundle);
        let out = serde_json::to_string_pretty(&bundle).unwrap();

        // Query text survives — that is the whole point of the flag.
        for kept in [
            "My Private Query",
            "my private query",
            "what is my ssn",
            "?q=my+private+query",
        ] {
            assert!(out.contains(kept), "{kept:?} missing:\n{out}");
        }
        // Secrets, credentials and free-form actors still never leave.
        for leaked in ["sk-live-secret", "engine-secret", "user:pass", "Jane Doe"] {
            assert!(!out.contains(leaked), "{leaked:?} leaked:\n{out}");
        }
        assert_eq!(bundle.sections["config"]["ai"]["api_key"], json!(REDACTED));
        assert_eq!(
            bundle.sections["config"]["engines"][0]["egress"]["proxy"],
            json!("http://proxy.internal:3128/")
        );
        // But a `?`-bearing URL keeps its query under verbose.
        let span = &bundle.sections["errors_tail"][1]["span"];
        assert_eq!(
            span["fields"]["url"],
            json!("https://bing.example/search?q=my+private+query")
        );
        // And text with a URL but nothing to scrub is byte-stable.
        assert_eq!(
            bundle.sections["engines"][1]["last_error"],
            json!("timeout")
        );
    }

    #[test]
    fn hash_is_deterministic_normalized_and_cache_key_shaped() {
        // Same digest regardless of casing/spacing, and the 64-hex shape
        // a `query_hash` column carries.
        let h = hash_query("Tan Stack");
        assert_eq!(h, hash_query("tan  stack"));
        assert_eq!(h.len(), 64);
        assert!(crate::CacheKey::from_str(&h).is_ok());
    }
}
