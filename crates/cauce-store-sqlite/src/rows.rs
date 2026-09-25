//! Row decoding: SQLite values back into `cauce-core` contract types.
//!
//! Conventions: timestamps are INTEGER unix epoch ms, `client` is the
//! `ClientKind::label()` string (`ui | api | mcp:<name> | cli`), enums are
//! their snake_case serde names, `tier` is 1-3, ids/uuids/urls are TEXT.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{
    AuditRow, BreakerState, CacheKey, CachedAnswer, CachedSearch, ClickRow, ClientKind,
    EngineHealthRow, EngineId, LogSource, SearchLogRow, SearchResponse, StoreError, Tier,
};
use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::de::DeserializeOwned;
use url::Url;
use uuid::Uuid;

/// Column list shared by every `cache_entries` read.
pub const CACHE_COLS: &str =
    "key, query, params_json, payload_json, created_at, expires_at, hits, engines_json";

/// Column list shared by every `answers` read.
pub const ANSWER_COLS: &str =
    "key, query, model, payload_json, sources_json, created_at, expires_at";

/// Wrap a `StoreError` for a rusqlite row closure; `as_store` unwraps it again
/// at the outer boundary so `Corrupt` is not flattened into `Backend`.
pub fn as_sql(e: StoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

/// Inverse of [`as_sql`]: recover the `StoreError` inside a conversion
/// failure, else report the SQL error as `Backend`.
pub fn as_store(e: rusqlite::Error) -> StoreError {
    match e {
        rusqlite::Error::FromSqlConversionFailure(_, _, inner)
        | rusqlite::Error::ToSqlConversionFailure(inner) => match inner.downcast::<StoreError>() {
            Ok(se) => *se,
            Err(inner) => StoreError::Backend(inner.to_string()),
        },
        other => StoreError::Backend(other.to_string()),
    }
}

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn to_ms(dt: &DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

pub fn from_ms(ms: i64) -> Result<DateTime<Utc>, StoreError> {
    DateTime::from_timestamp_millis(ms)
        .ok_or_else(|| StoreError::Corrupt(format!("timestamp out of range: {ms}")))
}

fn corrupt(col: &str, e: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(format!("{col}: {e}"))
}

fn json<T: DeserializeOwned>(row: &Row, idx: usize, col: &str) -> Result<T, StoreError> {
    let s: String = row.get(idx).map_err(|e| corrupt(col, e))?;
    serde_json::from_str(&s).map_err(|e| corrupt(col, e))
}

fn text(row: &Row, idx: usize, col: &str) -> Result<String, StoreError> {
    row.get(idx).map_err(|e| corrupt(col, e))
}

fn int(row: &Row, idx: usize, col: &str) -> Result<i64, StoreError> {
    row.get(idx).map_err(|e| corrupt(col, e))
}

fn opt_int(row: &Row, idx: usize, col: &str) -> Result<Option<i64>, StoreError> {
    row.get(idx).map_err(|e| corrupt(col, e))
}

fn opt_text(row: &Row, idx: usize, col: &str) -> Result<Option<String>, StoreError> {
    row.get(idx).map_err(|e| corrupt(col, e))
}

fn cache_key(s: &str) -> Result<CacheKey, StoreError> {
    s.parse().map_err(|e: String| StoreError::Corrupt(e))
}

/// `ClientKind::label()` inverse.
pub fn parse_client(s: &str) -> Result<ClientKind, StoreError> {
    match s {
        "ui" => Ok(ClientKind::Ui),
        "api" => Ok(ClientKind::Api),
        "cli" => Ok(ClientKind::Cli),
        s if s.starts_with("mcp:") => Ok(ClientKind::Mcp(s[4..].to_string())),
        other => Err(StoreError::Corrupt(format!(
            "unknown client label: {other}"
        ))),
    }
}

fn parse_source(s: &str) -> Result<LogSource, StoreError> {
    match s {
        "cache" => Ok(LogSource::Cache),
        "network" => Ok(LogSource::Network),
        other => Err(StoreError::Corrupt(format!("unknown log source: {other}"))),
    }
}

pub fn source_str(s: LogSource) -> &'static str {
    match s {
        LogSource::Cache => "cache",
        LogSource::Network => "network",
    }
}

fn parse_tier(v: i64) -> Result<Tier, StoreError> {
    Tier::try_from(v as u8).map_err(StoreError::Corrupt)
}

fn parse_breaker(s: &str) -> Result<BreakerState, StoreError> {
    match s {
        "closed" => Ok(BreakerState::Closed),
        "open" => Ok(BreakerState::Open),
        "half_open" => Ok(BreakerState::HalfOpen),
        other => Err(StoreError::Corrupt(format!(
            "unknown breaker state: {other}"
        ))),
    }
}

pub fn breaker_str(s: BreakerState) -> &'static str {
    match s {
        BreakerState::Closed => "closed",
        BreakerState::Open => "open",
        BreakerState::HalfOpen => "half_open",
    }
}

pub fn engines_to_json(ids: &[EngineId]) -> Result<String, StoreError> {
    serde_json::to_string(ids).map_err(StoreError::from)
}

/// Decode one `cache_entries` row selected with [`CACHE_COLS`].
pub fn cached(row: &Row) -> Result<CachedSearch, StoreError> {
    Ok(CachedSearch {
        key: cache_key(&text(row, 0, "key")?)?,
        query: text(row, 1, "query")?,
        params: json(row, 2, "params_json")?,
        response: {
            let payload: String = text(row, 3, "payload_json")?;
            serde_json::from_str::<SearchResponse>(&payload)
                .map_err(|e| corrupt("payload_json", e))?
        },
        created_at: from_ms(int(row, 4, "created_at")?)?,
        expires_at: from_ms(int(row, 5, "expires_at")?)?,
        hits: int(row, 6, "hits")? as u64,
        engines: json(row, 7, "engines_json")?,
    })
}

/// Decode one `answers` row selected with [`ANSWER_COLS`].
pub fn answer(row: &Row) -> Result<CachedAnswer, StoreError> {
    Ok(CachedAnswer {
        key: text(row, 0, "key")?.parse().map_err(StoreError::Corrupt)?,
        query: text(row, 1, "query")?,
        model: text(row, 2, "model")?,
        payload: json(row, 3, "payload_json")?,
        sources: json(row, 4, "sources_json")?,
        created_at: from_ms(int(row, 5, "created_at")?)?,
        expires_at: from_ms(int(row, 6, "expires_at")?)?,
    })
}

/// Decode one `search_log` row selected as
/// `id, ts, query_hash, query, client, source, tier, latency_ms, result_count, engines_json, deadline_hit, query_raw`.
pub fn search_log(row: &Row) -> Result<SearchLogRow, StoreError> {
    Ok(SearchLogRow {
        id: Some(int(row, 0, "id")?),
        ts: from_ms(int(row, 1, "ts")?)?,
        query_hash: cache_key(&text(row, 2, "query_hash")?)?,
        query: text(row, 3, "query")?,
        query_raw: opt_text(row, 11, "query_raw")?,
        client: parse_client(&text(row, 4, "client")?)?,
        source: parse_source(&text(row, 5, "source")?)?,
        tier: opt_int(row, 6, "tier")?.map(parse_tier).transpose()?,
        latency_ms: int(row, 7, "latency_ms")? as u32,
        result_count: int(row, 8, "result_count")? as u32,
        engines: json(row, 9, "engines_json")?,
        deadline_hit: int(row, 10, "deadline_hit")? != 0,
    })
}

/// Decode one `clicks` row selected as
/// `id, ts, query_hash, url, title, position, client`.
pub fn click(row: &Row) -> Result<ClickRow, StoreError> {
    Ok(ClickRow {
        id: Some(int(row, 0, "id")?),
        ts: from_ms(int(row, 1, "ts")?)?,
        query_hash: opt_text(row, 2, "query_hash")?
            .map(|s| cache_key(&s))
            .transpose()?,
        url: Url::parse(&text(row, 3, "url")?).map_err(|e| corrupt("url", e))?,
        title: text(row, 4, "title")?,
        position: int(row, 5, "position")? as u32,
        client: parse_client(&text(row, 6, "client")?)?,
    })
}

/// Decode one `engine_health` row selected as
/// `engine_id, ewma_ms, failures, breaker_state, breaker_until, last_ok_at, last_error`.
pub fn health(row: &Row) -> Result<EngineHealthRow, StoreError> {
    Ok(EngineHealthRow {
        engine: EngineId::from(text(row, 0, "engine_id")?),
        ewma_ms: row.get::<_, f64>(1).map_err(|e| corrupt("ewma_ms", e))?,
        failures: int(row, 2, "failures")? as u32,
        breaker: parse_breaker(&text(row, 3, "breaker_state")?)?,
        breaker_until: opt_int(row, 4, "breaker_until")?.map(from_ms).transpose()?,
        last_ok_at: opt_int(row, 5, "last_ok_at")?.map(from_ms).transpose()?,
        last_error: opt_text(row, 6, "last_error")?,
    })
}

/// Decode one `audit` row selected as
/// `id, ts, actor, action, target, details_json, request_id`.
pub fn audit(row: &Row) -> Result<AuditRow, StoreError> {
    Ok(AuditRow {
        id: Some(int(row, 0, "id")?),
        ts: from_ms(int(row, 1, "ts")?)?,
        actor: text(row, 2, "actor")?,
        action: text(row, 3, "action")?,
        target: text(row, 4, "target")?,
        details: json(row, 5, "details_json")?,
        request_id: opt_text(row, 6, "request_id")?
            .map(|s| Uuid::parse_str(&s).map_err(|e| corrupt("request_id", e)))
            .transpose()?,
    })
}
