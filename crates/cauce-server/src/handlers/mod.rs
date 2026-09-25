//! Route handlers for the wave-0 JSON surface.
//!
//! Every handler takes the [`RequestCtx`] extension the middleware installs
//! and returns errors through [`ApiError`], so every response (success or
//! not) is traceable by `request_id`.
//!
//! Module map (issue #154): `mod.rs` is the shared plumbing — [`QueryParams`]
//! query decoding, the [`cache_key`] parse, audit emission ([`write_audit`]),
//! the `config.put` diff walker [`changed_config_paths`], the SSE error
//! envelope [`search_error_payload`] and the engines-page [`engine_error_class`]
//! — plus the `MAX_LIMIT` page cap. The route handlers live beside it by
//! family: [`search`] — `/api/search` and the SSE stream; [`answer`] —
//! `POST /api/answer`, the W4-03 grounded-answer SSE route (`ai` builds);
//! [`suggest`] — `/api/suggest` OpenSearch completions; [`cache`] —
//! `/api/cache` listing, get and deletes; [`engines`] — `/api/engines`,
//! reset and enable/disable; [`history`] — `/api/history`, click, stats,
//! metrics, health and audit; [`config`] — `/api/config` get/put.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use axum::http::HeaderMap;
use cauce_core::{AuditRow, CacheKey, EngineError, EngineId, PipelineError, SearchRequest, Store};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use crate::error::ApiError;
use crate::middleware::RequestCtx;
use crate::observability::audit;

#[cfg(feature = "ai")]
mod answer;
mod cache;
mod config;
mod engines;
mod history;
mod search;
mod suggest;

#[cfg(feature = "ai")]
pub use answer::answer;
pub(crate) use cache::cache_list_data;
pub use cache::{cache_bulk_delete, cache_delete, cache_get, cache_list};
pub use config::{config_get, config_put};
pub(crate) use engines::engine_views;
pub use engines::{EngineView, engine_disable, engine_enable, engine_reset, engines_list};
pub(crate) use history::{HISTORY_LIMIT, audit_list_data, history_inner};
pub use history::{audit_list, click, health, history, history_delete, metrics, stats};
pub(crate) use search::{parse_search_request, search_error, search_inner, search_inner_classed};
pub use search::{search, search_stream};
pub use suggest::suggest;

/// Cap on caller-supplied `limit`/`offset`-style page sizes.
pub(super) const MAX_LIMIT: u32 = 1_000;

pub fn search_error_payload(ctx: &RequestCtx, req: &SearchRequest, error: PipelineError) -> Value {
    search::search_error(ctx, req, error).envelope()
}

/// The error class an engines-page test query renders for an all-failed
/// fan-out: the single engine's [`EngineError`] class when exactly one
/// failed (a pinned test), `upstream failed` otherwise.
#[cfg_attr(not(feature = "ui"), allow(dead_code))]
pub(super) fn engine_error_class(failures: &[(EngineId, EngineError)]) -> &'static str {
    use crate::strings::engines as copy;

    if failures.len() != 1 {
        return copy::TEST_UPSTREAM;
    }
    match failures[0].1 {
        EngineError::Blocked => copy::TEST_BLOCKED,
        EngineError::Timeout => copy::TEST_TIMEOUT,
        EngineError::NoResults => copy::TEST_NO_RESULTS,
        EngineError::RateLimited => copy::TEST_RATE_LIMITED,
        EngineError::Parse(_) => copy::TEST_PARSE,
        EngineError::Transport(_) => copy::TEST_TRANSPORT,
    }
}

/// Emit + persist one audit row (observability helper: JSONL event first,
/// `audit` table write after). `actor` honours `X-Actor`.
pub(super) async fn write_audit(
    store: &Arc<dyn Store>,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    action: &str,
    target: String,
    details: Value,
) -> Result<(), ApiError> {
    audit(
        store.as_ref(),
        AuditRow {
            id: None,
            ts: Utc::now(),
            actor: ctx.actor(headers),
            action: action.to_string(),
            target,
            details,
            request_id: Some(ctx.request_id.as_uuid()),
        },
    )
    .await
    .map_err(|e| ctx.store(&e))
}

pub(super) fn cache_key(ctx: &RequestCtx, raw: &str) -> Result<CacheKey, ApiError> {
    raw.parse::<CacheKey>().map_err(|e| ctx.bad_request(e))
}

/// A query string decoded into ordered `(key, value)` pairs. Decoding is
/// `url::form_urlencoded` (percent-escapes, `+` for space); duplicates and
/// unknown keys are 400s instead of silent surprises.
pub(crate) struct QueryParams(Vec<(String, String)>);

impl QueryParams {
    pub(crate) fn parse(raw: Option<&str>, ctx: &RequestCtx) -> Result<Self, ApiError> {
        let mut pairs = Vec::new();
        for (k, v) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
            if pairs.iter().any(|(seen, _)| *seen == k) {
                return Err(ctx.bad_request(format!("duplicate query parameter {k:?}")));
            }
            pairs.push((k.into_owned(), v.into_owned()));
        }
        Ok(Self(pairs))
    }

    /// 400 when a key outside `allowed` is present (the inbound
    /// `deny_unknown_fields` contract applied to the query string).
    pub(crate) fn allow(&self, ctx: &RequestCtx, allowed: &[&str]) -> Result<(), ApiError> {
        for (k, _) in &self.0 {
            if !allowed.contains(&k.as_str()) {
                return Err(ctx.bad_request(format!("unknown query parameter {k:?}")));
            }
        }
        Ok(())
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Present and non-blank: `q=` and whitespace-only values are 400s
    /// just like an absent parameter — a blank `q` would otherwise run a
    /// fan-out on the empty normalized query (#89).
    pub(crate) fn required<'a>(&'a self, ctx: &RequestCtx, key: &str) -> Result<&'a str, ApiError> {
        self.get(key)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| ctx.bad_request(format!("missing required parameter {key:?}")))
    }

    pub(crate) fn u32(&self, ctx: &RequestCtx, key: &str, default: u32) -> Result<u32, ApiError> {
        match self.get(key) {
            None => Ok(default),
            Some(v) => v
                .parse::<u32>()
                .map_err(|_| ctx.bad_request(format!("invalid {key} {v:?}: expected a u32"))),
        }
    }

    /// `page` is 1-based; `page=0` and non-numeric values are 400s.
    pub(crate) fn page(&self, ctx: &RequestCtx) -> Result<u8, ApiError> {
        match self.get("page") {
            None => Ok(1),
            Some(v) => v
                .parse::<u8>()
                .ok()
                .filter(|p| *p >= 1)
                .ok_or_else(|| ctx.bad_request(format!("invalid page {v:?}"))),
        }
    }

    /// Presence-style flag: `?expired`, `?expired=true|1|yes` are true;
    /// `?expired=false|0|no` is false; anything else is a 400.
    pub(crate) fn flag(&self, ctx: &RequestCtx, key: &str) -> Result<bool, ApiError> {
        match self.get(key) {
            None => Ok(false),
            Some(v) => match v.to_ascii_lowercase().as_str() {
                "" | "true" | "1" | "yes" => Ok(true),
                "false" | "0" | "no" => Ok(false),
                _ => Err(ctx.bad_request(format!("invalid {key} {v:?}: expected a boolean"))),
            },
        }
    }

    /// `since` accepts RFC 3339 (`2026-10-01T12:00:00Z`), a bare
    /// `YYYY-MM-DD` date (interpreted as that UTC midnight) or a relative
    /// window token — `24h`, `7d`, `30d` (W2-02's history filter set) or
    /// `all` (no lower bound).
    pub(crate) fn since(
        &self,
        ctx: &RequestCtx,
        key: &str,
    ) -> Result<Option<DateTime<Utc>>, ApiError> {
        let Some(v) = self.get(key) else {
            return Ok(None);
        };
        match v {
            "24h" => return Ok(Some(Utc::now() - chrono::Duration::hours(24))),
            "7d" => return Ok(Some(Utc::now() - chrono::Duration::days(7))),
            "30d" => return Ok(Some(Utc::now() - chrono::Duration::days(30))),
            "all" => return Ok(None),
            _ => {}
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
            return Ok(Some(dt.with_timezone(&Utc)));
        }
        if let Ok(day) = NaiveDate::parse_from_str(v, "%Y-%m-%d")
            && let Some(dt) = day.and_hms_opt(0, 0, 0)
        {
            return Ok(Some(dt.and_utc()));
        }
        Err(ctx.bad_request(format!(
            "invalid {key} {v:?}: expected RFC 3339, YYYY-MM-DD, or one of 24h|7d|30d|all"
        )))
    }
}

/// Dotted paths whose values differ between two raw config trees, for the
/// `config.put` audit detail. `[[engines]]` entries pair by `id` so a tier
/// edit reports `engines.ddgs.tier`, not the whole array.
pub(super) fn changed_config_paths(old: &toml::Value, new: &toml::Value) -> Vec<String> {
    fn walk(path: &str, a: &toml::Value, b: &toml::Value, out: &mut Vec<String>) {
        if path == "engines"
            && let (Some(ea), Some(eb)) = (a.as_array(), b.as_array())
        {
            let ids: std::collections::BTreeSet<String> = ea
                .iter()
                .chain(eb.iter())
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect();
            for id in ids {
                fn find_engine<'v>(arr: &'v [toml::Value], id: &str) -> Option<&'v toml::Value> {
                    arr.iter()
                        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(id))
                }
                match (find_engine(ea, &id), find_engine(eb, &id)) {
                    (Some(va), Some(vb)) => walk(&format!("engines.{id}"), va, vb, out),
                    (entry_a, entry_b) => {
                        if entry_a.is_some() != entry_b.is_some() {
                            out.push(format!("engines.{id}"));
                        }
                    }
                }
            }
            return;
        }
        match (a.as_table(), b.as_table()) {
            (Some(ta), Some(tb)) => {
                let keys: std::collections::BTreeSet<&String> =
                    ta.keys().chain(tb.keys()).collect();
                for k in keys {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    match (ta.get(k), tb.get(k)) {
                        (Some(va), Some(vb)) => walk(&p, va, vb, out),
                        _ => out.push(p),
                    }
                }
            }
            _ => {
                if a != b {
                    out.push(path.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk("", old, new, &mut out);
    out
}
