//! Typed configuration, XDG directories, `${...}` interpolation and
//! resource-adaptive defaults (`Resources`).
//!
//! Module map (issue #158): `mod.rs` is the public API — [`ConfigError`],
//! [`EnvMap`], [`Dirs`], the typed section structs, [`EngineKind`],
//! [`EngineEntry`] and the [`Config`] facade (load, `from_raw`
//! validation, `save`, the redacted display tree and
//! `restore_redacted`). Beside it: [`tree`] — TOML tree plumbing
//! (`set_path`, `tree_at`, `env_scalar`, `default_tree`);
//! [`interpolate`] — `${env:...}`/`${file:...}`/`$$` expansion;
//! [`redact`] — secret-leaf redaction for the display tree;
//! [`resources`] — host memory/core detection and scaled defaults.
//!
//! Settled contract (`.agents/plans/v3/wave-0-skeleton.md`, "Settled inputs"):
//! TOML at `$CAUCE_CONFIG_DIR/config.toml` (default `~/.config/cauce/`), data at
//! `$CAUCE_DATA_DIR` (default `~/.local/share/cauce/`: `cauce.db`, `logs/`).
//! Interpolation on string values at load: `${env:NAME}` (missing is an
//! error), `${env:NAME:-default}`, `${env:NAME:?msg}` (missing is an error
//! carrying `msg`), `${file:PATH}`, and `$$` as a literal `$`. The `:`
//! forms follow POSIX: unset-or-empty counts as missing. `Config::save`
//! writes the raw template tree back, never resolved secrets. Precedence
//! is defaults < file < `CAUCE_*` env.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::{ENGINE_ID_PATTERN, EngineId, Tier};

mod interpolate;
mod redact;
mod resources;
#[cfg(test)]
mod tests_support;
mod tree;

pub use interpolate::interpolate_str;
pub use resources::Resources;

use interpolate::interpolate_tree;
use redact::{REDACTED, SECRET_PATHS, redact_secret_paths, redacted_leaves};
use tree::{default_tree, env_scalar, set_display, set_path, to_value, tree_at, tree_mut_at};

/// An environment map: the real process-env snapshot in production, an
/// explicit map in tests so loads stay deterministic.
pub type EnvMap = BTreeMap<String, String>;

/// Snapshot of the process environment. Uses `vars_os` and skips non-UTF-8
/// entries: `std::env::vars()` panics on those, which would take the whole
/// process down at startup.
pub fn system_env() -> EnvMap {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}

/// `CAUCE_*` variables applied on top of the file layer (precedence env >
/// file > defaults). Each entry is `(env name, dotted TOML path, parse as
/// scalar)`; `parse` converts `true`/`false`/integers/floats before the
/// value is overlaid, the rest stay strings.
const ENV_OVERRIDES: &[(&str, &[&str], bool)] = &[
    ("CAUCE_SERVER_HOST", &["server", "host"], false),
    ("CAUCE_SERVER_PORT", &["server", "port"], true),
    ("CAUCE_SERVER_PUBLIC_URL", &["server", "public_url"], false),
    ("CAUCE_SEARCH_DEADLINE_MS", &["search", "deadline_ms"], true),
    ("CAUCE_SEARCH_MIN_RESULTS", &["search", "min_results"], true),
    (
        "CAUCE_SEARCH_HEDGE_FLOOR_MS",
        &["search", "hedge_floor_ms"],
        true,
    ),
    (
        "CAUCE_SEARCH_HEDGE_CEILING_MS",
        &["search", "hedge_ceiling_ms"],
        true,
    ),
    ("CAUCE_SEARCH_TTL_S", &["search", "ttl_s"], true),
    ("CAUCE_SEARCH_TTL_CAP_S", &["search", "ttl_cap_s"], true),
    (
        "CAUCE_CACHE_STALE_GRACE_S",
        &["cache", "stale_grace_s"],
        true,
    ),
    (
        "CAUCE_CACHE_DEGRADED_TTL_S",
        &["cache", "degraded_ttl_s"],
        true,
    ),
    (
        "CAUCE_ADMISSION_MAX_WAIT_MS",
        &["admission", "max_wait_ms"],
        true,
    ),
    (
        "CAUCE_ADMISSION_MAX_CONCURRENT_PER_ENGINE",
        &["admission", "max_concurrent_per_engine"],
        true,
    ),
    (
        "CAUCE_HEALTH_DEGRADED_THRESHOLD",
        &["health", "degraded_threshold"],
        true,
    ),
    (
        "CAUCE_HEALTH_DEGRADED_WINDOW_S",
        &["health", "degraded_window_s"],
        true,
    ),
    ("CAUCE_MERGE_RRF_K", &["merge", "rrf_k"], true),
    (
        "CAUCE_MERGE_COLLAPSE_SAME_HOST_AFTER",
        &["merge", "collapse_same_host_after"],
        true,
    ),
    (
        "CAUCE_LOGS_RETENTION_DAYS",
        &["logs", "retention_days"],
        true,
    ),
    ("CAUCE_AI_BASE_URL", &["ai", "base_url"], false),
    ("CAUCE_AI_API_KEY", &["ai", "api_key"], false),
    ("CAUCE_AI_MODEL", &["ai", "model"], false),
    ("CAUCE_AI_ENABLED", &["ai", "enabled"], true),
    ("CAUCE_AI_PROTOCOL", &["ai", "protocol"], false),
    (
        "CAUCE_ARCHIVE_INDEX_ON_CLICK",
        &["archive", "index_on_click"],
        true,
    ),
    (
        "CAUCE_ARCHIVE_REQUESTS_PER_SECOND",
        &["archive", "requests_per_second"],
        true,
    ),
    ("CAUCE_ARCHIVE_BURST", &["archive", "burst"], true),
    (
        "CAUCE_ARCHIVE_ALLOW_PRIVATE",
        &["archive", "allow_private"],
        true,
    ),
    (
        "CAUCE_CONFIG_INTERPOLATION",
        &["config", "interpolation"],
        true,
    ),
];

// Reserved `CAUCE_*` variables that are not config overrides: they steer
// directories (`CAUCE_CONFIG_DIR`, `CAUCE_DATA_DIR`), the enabled engine set
// (`CAUCE_ENGINES`) or other subsystems (`CAUCE_REPLAY_*`, `CAUCE_LOG_PRETTY`,
// `CAUCE_LIVE`, `CAUCE_NIGHTLY`).

/// Errors from `Config::load`/`Config::save`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The config file exists but could not be read.
    #[error("cannot read config file {}: {source}", .path.display())]
    Read { path: PathBuf, source: io::Error },
    /// The config file is not valid TOML.
    #[error("cannot parse config file {}: {source}", .path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    /// The merged tree does not match the `Config` schema (unknown keys,
    /// wrong types).
    #[error("invalid config: {0}")]
    Invalid(#[source] toml::de::Error),
    /// `${env:NAME}` with `NAME` unset.
    #[error("{path}: environment variable {var} is not set")]
    MissingEnv { path: String, var: String },
    /// `${env:NAME:?msg}` with `NAME` unset; the error carries `msg`.
    #[error("{path}: environment variable {var} is not set: {msg}")]
    MissingEnvMsg {
        path: String,
        var: String,
        msg: String,
    },
    /// `${file:PATH}` where `PATH` is missing or unreadable.
    #[error("{path}: cannot read {}: {source}", .file.display())]
    MissingFile {
        path: String,
        file: PathBuf,
        source: io::Error,
    },
    /// Malformed or unknown `${...}` expression (includes unterminated `${`).
    #[error("{path}: invalid interpolation {expr:?}")]
    BadInterpolation { path: String, expr: String },
    /// `CAUCE_ENGINES` named an engine with no configured or built-in entry.
    #[error("CAUCE_ENGINES names unknown engine {0:?}")]
    UnknownEngine(String),
    /// A field value outside its allowed range (semantic, post-schema).
    #[error("{path}: {msg}")]
    InvalidValue { path: String, msg: String },
    /// An `[[engines]]` entry is inconsistent (e.g. `kind = "exec"` without
    /// a `command`).
    #[error("invalid engine entry {id:?}: {msg}")]
    InvalidEngine { id: String, msg: String },
    /// `Config::save` could not write the file.
    #[error("cannot write config file {}: {source}", .path.display())]
    Write { path: PathBuf, source: io::Error },
    /// Serialisation failure while saving or rendering.
    #[error("cannot serialize config: {0}")]
    Encode(#[source] toml::ser::Error),
}

/// Resolved filesystem locations. `CAUCE_CONFIG_DIR`/`CAUCE_DATA_DIR` win, then
/// `XDG_CONFIG_HOME`/`XDG_DATA_HOME`, then `~/.config/cauce` and
/// `~/.local/share/cauce`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// Directory holding `config.toml`.
    pub config_dir: PathBuf,
    /// Data directory holding `cauce.db` and `logs/`.
    pub data_dir: PathBuf,
}

impl Dirs {
    /// Resolve from the process environment.
    pub fn detect() -> Self {
        Self::detect_with(&system_env())
    }

    fn detect_with(env: &EnvMap) -> Self {
        let config_dir = env
            .get("CAUCE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                env.get("XDG_CONFIG_HOME")
                    .map(|x| Path::new(x).join("cauce"))
            })
            .unwrap_or_else(|| home_dir(env).join(".config/cauce"));
        let data_dir = env
            .get("CAUCE_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| env.get("XDG_DATA_HOME").map(|x| Path::new(x).join("cauce")))
            .unwrap_or_else(|| home_dir(env).join(".local/share/cauce"));
        Self {
            config_dir,
            data_dir,
        }
    }

    /// `$config_dir/config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// `$data_dir/cauce.db`.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("cauce.db")
    }

    /// `$data_dir/logs`.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

impl Default for Dirs {
    fn default() -> Self {
        Self::detect()
    }
}

fn home_dir(env: &EnvMap) -> PathBuf {
    env.get("HOME")
        .map(PathBuf::from)
        .or_else(std::env::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `[server]`: HTTP bind address. Loopback by default (settled inputs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Bind host; `127.0.0.1` unless explicitly exposed.
    #[serde(default = "default_host")]
    pub host: String,
    /// Bind port; 4479 by default.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Canonical externally visible HTTP(S) origin for absolute browser URLs.
    /// When unset, the effective bind host and port are used over HTTP.
    #[serde(default)]
    pub public_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            public_url: None,
        }
    }
}

impl ServerConfig {
    /// Absolute origin used by the OpenSearch descriptor. `public_url` is
    /// validated by `Config::from_raw`; request headers are never consulted.
    pub fn public_origin(&self, bind_host: &str, bind_port: u16) -> String {
        if let Some(public_url) = &self.public_url {
            return url::Url::parse(public_url)
                .expect("ServerConfig::public_url is validated when loaded")
                .origin()
                .ascii_serialization();
        }

        let bind_host = match bind_host.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V6(_)) => format!("[{bind_host}]"),
            _ => bind_host.to_string(),
        };
        let fallback = format!("http://{bind_host}:{bind_port}");
        url::Url::parse(&fallback)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_else(|_| format!("http://127.0.0.1:{bind_port}"))
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    4479
}

/// Validate the configured canonical origin before it can reach XML output.
fn validate_public_url(value: &str) -> Result<(), String> {
    if value
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\'' | '"'))
    {
        return Err("expected an HTTP(S) origin without whitespace, controls, or quotes".into());
    }
    let url =
        url::Url::parse(value).map_err(|e| format!("expected an absolute HTTP(S) origin: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("scheme must be http or https".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("origin must contain a valid host".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("username and password are not allowed".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("query and fragment are not allowed".into());
    }
    let (_, authority_and_path) = value
        .split_once("://")
        .ok_or_else(|| "expected an absolute hierarchical URL with an authority".to_string())?;
    let suffix_start = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    if !matches!(&authority_and_path[suffix_start..], "" | "/") || url.path() != "/" {
        return Err("path must be empty or /".into());
    }
    Ok(())
}

/// `[auth]`: the admin-auth switch (W1-13). The token mechanism itself is
/// deferred to `v3/later/postgres-and-multi-instance.md`; until it lands,
/// `enabled` is forced by the bind address — off on loopback, required off
/// it, so `cauce serve` refuses a non-loopback bind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Master switch. Forced `true` when the bind is not loopback (see
    /// [`AuthConfig::enabled_for`]); ignored on loopback until token auth
    /// exists, so setting it only earns a startup warning.
    #[serde(default)]
    pub enabled: bool,
}

impl AuthConfig {
    /// The effective value for a bind to `host`: forced on off-loopback,
    /// mirroring the former W6-03 line (`later/postgres-and-multi-instance.md`).
    /// W1-13 enforces the forced case by refusing the bind.
    pub fn enabled_for(&self, bind_host: &str) -> bool {
        self.enabled || !is_loopback_host(bind_host)
    }
}

/// The host part of an authority string (`host`, `host:port`, `[v6]`,
/// `[v6]:port`), without a trailing root-zone dot. Bare IPv6 literals
/// (more than one `:`) are returned whole.
pub fn host_part(authority: &str) -> &str {
    let a = authority.trim();
    if let Some(rest) = a.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    let h = if a.matches(':').count() > 1 {
        a // bare IPv6 literal; it cannot carry a `:port` suffix
    } else {
        a.rsplit_once(':').map(|(h, _)| h).unwrap_or(a)
    };
    h.trim_end_matches('.')
}

/// Loopback check shared by the `serve` bind refusal and the server's
/// Host/Origin guard (W1-13): `localhost`, any `*.localhost` alias (the
/// portless names), the whole `127.0.0.0/8` block and `::1`. Accepts both
/// bare hosts and `host:port` / `[v6]:port` authority forms. Wildcard
/// binds (`0.0.0.0`, `::`) are not loopback.
pub fn is_loopback_host(authority: &str) -> bool {
    let h = host_part(authority);
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    h.eq_ignore_ascii_case("localhost") || h.to_ascii_lowercase().ends_with(".localhost")
}

/// `[search]`: pipeline tunables (parent plan sections 3 and 4.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    /// Hard fan-out deadline in milliseconds.
    #[serde(default = "default_deadline_ms")]
    pub deadline_ms: u64,
    /// Results wanted before the hedge point is considered satisfied.
    #[serde(default = "default_min_results")]
    pub min_results: u32,
    /// Earliest tier-2 hedge point in ms (W3-01): tier-1 gets at least
    /// this long to answer before the hedge can fire.
    #[serde(default = "default_hedge_floor_ms")]
    pub hedge_floor_ms: u64,
    /// Latest tier-2 hedge point in ms (W3-01): a slow tier-1 history
    /// never delays the hedge past this.
    #[serde(default = "default_hedge_ceiling_ms")]
    pub hedge_ceiling_ms: u64,
    /// Default cache TTL in seconds.
    #[serde(default = "default_ttl_s")]
    pub ttl_s: u64,
    /// Upper bound for per-request `ttl_s` overrides.
    #[serde(default = "default_ttl_cap_s")]
    pub ttl_cap_s: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            deadline_ms: default_deadline_ms(),
            min_results: default_min_results(),
            hedge_floor_ms: default_hedge_floor_ms(),
            hedge_ceiling_ms: default_hedge_ceiling_ms(),
            ttl_s: default_ttl_s(),
            ttl_cap_s: default_ttl_cap_s(),
        }
    }
}

fn default_deadline_ms() -> u64 {
    3000
}

fn default_min_results() -> u32 {
    5
}

fn default_hedge_floor_ms() -> u64 {
    300
}

fn default_hedge_ceiling_ms() -> u64 {
    1500
}

fn default_ttl_s() -> u64 {
    3600
}

fn default_ttl_cap_s() -> u64 {
    86400
}

/// `[admission]`: singleflight + bounded per-engine queue (parent plan
/// 6.2, W1-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionConfig {
    /// Total milliseconds a request may wait for per-engine slots before
    /// admission overflows to a stale row or a 429. Default 1500.
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
    /// Concurrent upstream calls allowed per engine id. Default 3 matches
    /// the per-engine politeness burst (1 req/s burst 3).
    #[serde(default = "default_max_concurrent_per_engine")]
    pub max_concurrent_per_engine: u32,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            max_wait_ms: default_max_wait_ms(),
            max_concurrent_per_engine: default_max_concurrent_per_engine(),
        }
    }
}

fn default_max_wait_ms() -> u64 {
    1500
}

fn default_max_concurrent_per_engine() -> u32 {
    3
}

/// `[health]`: circuit-breaker knobs (W3-07). The other breaker rules
/// (`RateLimited`/`Blocked` abuse window, the timeout streak) stay
/// settled constants on `HealthPolicy`; this section holds the degraded
/// pair: `degraded_threshold` consecutive `Parse`/`Transport` errors
/// open the breaker for `degraded_window_s` seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthConfig {
    /// Consecutive `Parse`/`Transport` errors that open the breaker
    /// (default 5). Must be >= 1: `0` would fire on the very first error
    /// instead of being a streak.
    #[serde(default = "default_degraded_threshold")]
    pub degraded_threshold: u32,
    /// Seconds the breaker stays open once the degraded streak trips it
    /// (default 600) — also the re-open window for a half-open probe
    /// that fails with `Parse`/`Transport`. `0` makes the breaker never
    /// stay open (it flips to `HalfOpen` on the next gate).
    #[serde(default = "default_degraded_window_s")]
    pub degraded_window_s: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            degraded_threshold: default_degraded_threshold(),
            degraded_window_s: default_degraded_window_s(),
        }
    }
}

fn default_degraded_threshold() -> u32 {
    5
}

fn default_degraded_window_s() -> u64 {
    600
}

/// `[cache]`: cache-tier behaviour beyond TTLs (those live in `[search]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// `[cache.lexical]`: the tier-2 FTS lookup (W1-10).
    #[serde(default)]
    pub lexical: LexicalConfig,
    /// Seconds past `expires_at` a tier-1 row may still be served stale
    /// while a deduped background refresh re-fetches it (W3-02).
    /// `0` disables the stale serve; expired rows are only evicted once
    /// they are older than this window.
    #[serde(default = "default_stale_grace_s")]
    pub stale_grace_s: u64,
    /// TTL (seconds) applied to a response whose fan-out was partial —
    /// any engine `Failed` or the deadline hit (W3-02): a degraded
    /// answer never earns the full `search.ttl_s`.
    #[serde(default = "default_degraded_ttl_s")]
    pub degraded_ttl_s: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            lexical: LexicalConfig::default(),
            stale_grace_s: default_stale_grace_s(),
            degraded_ttl_s: default_degraded_ttl_s(),
        }
    }
}

fn default_stale_grace_s() -> u64 {
    6 * 3600
}

fn default_degraded_ttl_s() -> u64 {
    60
}

/// `[cache.lexical]`: tier-2 acceptance gate (W1-10). On a tier-1 miss the
/// pipeline FTS-matches stored entries and serves the best-ranked row whose
/// query shares `threshold` of the request's tokens (Jaccard after
/// normalisation and stopword removal) under the same page/lang.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalConfig {
    /// Master switch for the tier-2 lookup.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Minimum Jaccard token similarity to accept a hit. Must be finite
    /// and in `(0.0, 1.0]`; `Config::load` rejects anything else.
    #[serde(default = "default_lexical_threshold")]
    pub threshold: f64,
}

impl Default for LexicalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: default_lexical_threshold(),
        }
    }
}

fn default_lexical_threshold() -> f64 {
    0.8
}

/// `[merge]` section: RRF merge tuning (W3-03). The reliability weight
/// itself is not a knob — it is computed from engine health; this section
/// holds the RRF constant and the host-diversity cap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeConfig {
    /// The RRF constant: `score += weight / (rrf_k + rank)` per engine.
    /// Must be finite and `>= 0.0`; `Config::load` rejects anything else.
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f64,
    /// Max merged results emitted per host — `m.`/`amp.` folds share the
    /// host. `0` disables the collapse.
    #[serde(default = "default_collapse_same_host_after")]
    pub collapse_same_host_after: u32,
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            rrf_k: default_rrf_k(),
            collapse_same_host_after: default_collapse_same_host_after(),
        }
    }
}

fn default_rrf_k() -> f64 {
    60.0
}

fn default_collapse_same_host_after() -> u32 {
    3
}

/// `[logs]`: JSONL log retention (W0-05 consumes `retention_days`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsConfig {
    /// Days a `logs/cauce-YYYY-MM-DD.jsonl` file is kept.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for LogsConfig {
    fn default() -> Self {
        Self {
            retention_days: default_retention_days(),
        }
    }
}

fn default_retention_days() -> u32 {
    7
}

/// `[ai]`: provider settings. Disabled until wave 4 and blank by default —
/// `base_url`/`api_key` are empty strings, not a reference to anyone's
/// local gateway; point them at an OpenAI-compatible endpoint (e.g. an
/// `api_key = "${env:...}"` template) to use them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    /// OpenAI-compatible endpoint.
    #[serde(default)]
    pub base_url: String,
    /// API key or a `${env:...}`/`${file:...}` template.
    #[serde(default)]
    pub api_key: String,
    /// Provider model name (W4 settled inputs); empty until one is chosen.
    #[serde(default)]
    pub model: String,
    /// Master switch; `false` until W4.
    #[serde(default)]
    pub enabled: bool,
    /// Wire protocol the provider client speaks (W4 settled inputs):
    /// `openai` (default, `chat/completions`) or `anthropic` (`/v1/messages`).
    #[serde(default)]
    pub protocol: AiProtocol,
}

/// The `[ai].protocol` vocabulary (W4-05): which provider client the
/// AI surface builds. An unknown value fails config load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProtocol {
    /// OpenAI-compatible `POST {base_url}/chat/completions` (W4-01).
    #[default]
    #[serde(rename = "openai")]
    OpenAi,
    /// Anthropic Messages API `POST {base_url}/v1/messages` (W4-05).
    #[serde(rename = "anthropic")]
    Anthropic,
}

impl AiProtocol {
    /// The config-file spelling — also the eval transcript suffix.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

impl std::fmt::Display for AiProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `[archive]` (W5-01): the fetch-and-index pipeline behind
/// `POST /api/pages`, the UI click beacon and MCP `fetch_and_index`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveConfig {
    /// Whether clicking a result link in the UI fires the indexing beacon
    /// (default true, settled input). The beacon is failure-silent and
    /// never blocks the navigation either way.
    #[serde(default = "default_true")]
    pub index_on_click: bool,
    /// Host-keyed token-bucket refill rate for archive fetches (>= 1).
    /// Conservative by default so archive traffic never starves engine
    /// fan-out.
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u32,
    /// Host-keyed token-bucket burst capacity (>= 1).
    #[serde(default = "default_archive_burst")]
    pub burst: u32,
    /// Skip the SSRF egress guard (default false): when true the fetcher
    /// may dial private/reserved addresses (loopback, RFC 1918,
    /// link-local). A documented opt-in for indexing local services —
    /// `POST /api/pages` and MCP `fetch_and_index` stay open to
    /// server-side request forgery while it is on.
    #[serde(default)]
    pub allow_private: bool,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            index_on_click: true,
            requests_per_second: default_requests_per_second(),
            burst: default_archive_burst(),
            allow_private: false,
        }
    }
}

fn default_archive_burst() -> u32 {
    2
}

/// `[config]`: meta settings about the config file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaConfig {
    /// Whether `${...}` interpolation runs at load. `false` is the
    /// enterprise escape hatch (literal strings everywhere).
    #[serde(default = "default_true")]
    pub interpolation: bool,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            interpolation: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Runtime kind of an `[[engines]]` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// YAML-spec engine (W1 runtimes).
    Declarative,
    /// Subprocess speaking the exec protocol (parent plan 4.3).
    Exec,
    /// Cassette/synthetic engine for tests and demos.
    Replay,
}

/// `[engines.egress]` (a sub-table of an `[[engines]]` entry): upstream
/// egress and politeness policy for that engine's HTTP calls (W1-01).
/// Absent means a direct connection with a token bucket of 1 req/s,
/// burst 3 (settled inputs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EgressConfig {
    /// Static proxy URL (`http://`, `https://`, `socks5://`,
    /// `socks5h://`). Absent = direct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// Token-bucket refill rate in requests per second (>= 1).
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u32,
    /// Token-bucket burst capacity (>= 1).
    #[serde(default = "default_burst")]
    pub burst: u32,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            proxy: None,
            requests_per_second: default_requests_per_second(),
            burst: default_burst(),
        }
    }
}

fn default_requests_per_second() -> u32 {
    1
}

fn default_burst() -> u32 {
    3
}

/// One `[[engines]]` table. `command`/`args`/`env`/`cwd` describe the child
/// for `kind = "exec"`; `spec` points at the YAML file for
/// `kind = "declarative"`.
///
/// Field order matters for TOML serialisation: scalars and plain arrays
/// first, the `egress`/`env` tables last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineEntry {
    /// Stable id (`ddgs`, `replay`, `bing`, ...).
    pub id: EngineId,
    /// Runtime that executes this entry.
    pub kind: EngineKind,
    /// Member of the default fan-out set unless `CAUCE_ENGINES` overrides.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Executable to spawn (`exec` kind; required there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Arguments for `command`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Working directory of the child; relative `args` resolve against it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// YAML spec path (`declarative` kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    /// Fan-out tier override (`Tier` serializes as 1/2/3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
    /// Results per page the engine reports back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u8>,
    /// `[engines.egress]` sub-table: proxy and token-bucket policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<EgressConfig>,
    /// Extra environment on top of the inherited one (`exec` kind).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// `[engines.params]` table: static per-engine params forwarded to `exec`
    /// children on every v2-protocol request (issue #88).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

/// Built-in entries that always exist: `replay` (test engine, off unless
/// pinned so synthetic results never mix into real searches) and `ddgs`
/// (the day-1 real bridge). A file entry with the same id wins.
fn builtin_engines() -> Vec<EngineEntry> {
    vec![
        EngineEntry {
            id: EngineId::new("replay"),
            kind: EngineKind::Replay,
            enabled: false,
            command: None,
            args: vec![],
            cwd: None,
            spec: None,
            tier: None,
            page_size: None,
            egress: None,
            env: BTreeMap::new(),
            params: BTreeMap::new(),
        },
        EngineEntry {
            id: EngineId::new("ddgs"),
            kind: EngineKind::Exec,
            enabled: true,
            command: Some("python3".to_string()),
            args: vec!["sdk/python/cauce_engine_sdk/ddgs_auto.py".to_string()],
            cwd: None,
            spec: None,
            tier: Some(Tier::T2),
            page_size: Some(10),
            egress: None,
            env: BTreeMap::new(),
            params: BTreeMap::new(),
        },
    ]
}

/// The resolved configuration: defaults < TOML file < `CAUCE_*` env.
///
/// Besides the typed sections it keeps the raw (pre-interpolation) file
/// tree for `save` and a map of template paths for redacted display; none
/// of that is part of the TOML schema (`#[serde(skip)]`).
///
/// `Debug` and `Serialize` are manual: both emit the *redacted* view
/// (`display_tree`), so a resolved secret (e.g. `ai.api_key` from
/// `${env:PROVIDER_API_KEY}`) can never leak through `format!("{cfg:?}")`,
/// `serde_json::to_string(&cfg)` or a debug log line.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `[server]` section.
    #[serde(default)]
    pub server: ServerConfig,
    /// `[search]` section.
    #[serde(default)]
    pub search: SearchConfig,
    /// `[admission]` section.
    #[serde(default)]
    pub admission: AdmissionConfig,
    /// `[health]` section.
    #[serde(default)]
    pub health: HealthConfig,
    /// `[cache]` section.
    #[serde(default)]
    pub cache: CacheConfig,
    /// `[merge]` section.
    #[serde(default)]
    pub merge: MergeConfig,
    /// `[logs]` section.
    #[serde(default)]
    pub logs: LogsConfig,
    /// `[ai]` section.
    #[serde(default)]
    pub ai: AiConfig,
    /// `[archive]` section.
    #[serde(default)]
    pub archive: ArchiveConfig,
    /// `[auth]` section.
    #[serde(default)]
    pub auth: AuthConfig,
    /// `[[engines]]` entries plus the built-ins (`replay`, `ddgs`).
    #[serde(default = "builtin_engines")]
    pub engines: Vec<EngineEntry>,
    /// `[config]` section.
    #[serde(default)]
    pub config: MetaConfig,
    #[serde(skip)]
    dirs: Dirs,
    /// The file layer exactly as written (no interpolation applied), so
    /// `save` round-trips templates. Defaults tree when no file was loaded.
    #[serde(skip)]
    raw: Option<toml::Value>,
    /// Dotted paths of string values interpolation rewrote, mapped to their
    /// raw text; used by `display_tree` so resolved secrets never print.
    #[serde(skip)]
    templates: BTreeMap<Vec<String>, String>,
}

/// Serialization view of the typed sections only (no `dirs`/`raw`/
/// `templates`). Private: every public output path (`Debug`, `Serialize`,
/// `display_*`) goes through template redaction.
#[derive(Serialize)]
struct ConfigSections<'a> {
    server: &'a ServerConfig,
    search: &'a SearchConfig,
    admission: &'a AdmissionConfig,
    health: &'a HealthConfig,
    cache: &'a CacheConfig,
    merge: &'a MergeConfig,
    logs: &'a LogsConfig,
    ai: &'a AiConfig,
    archive: &'a ArchiveConfig,
    auth: &'a AuthConfig,
    engines: &'a [EngineEntry],
    config: &'a MetaConfig,
}

impl fmt::Debug for Config {
    /// Prints the redacted TOML, same as `cauce config show`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.display_toml() {
            Ok(text) => f.write_str(&text),
            Err(_) => f.write_str("<config display error>"),
        }
    }
}

impl Serialize for Config {
    /// Serializes the redacted tree: values produced by interpolation
    /// templates come out as their literal `${...}` text, so resolved
    /// secrets never reach a serializer (e.g. `GET /api/config`, W0-09).
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.display_tree()
            .map_err(serde::ser::Error::custom)?
            .serialize(s)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            search: SearchConfig::default(),
            admission: AdmissionConfig::default(),
            health: HealthConfig::default(),
            cache: CacheConfig::default(),
            merge: MergeConfig::default(),
            logs: LogsConfig::default(),
            ai: AiConfig::default(),
            archive: ArchiveConfig::default(),
            auth: AuthConfig::default(),
            engines: builtin_engines(),
            config: MetaConfig::default(),
            dirs: Dirs::default(),
            raw: None,
            templates: BTreeMap::new(),
        }
    }
}

impl Config {
    /// Load with the precedence chain defaults < `$CAUCE_CONFIG_DIR/config.toml`
    /// < `CAUCE_*` env. A missing file is not an error; a malformed or
    /// uninterpolatable one is (so `${env:NAME:?msg}` fails startup).
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_with(&system_env())
    }

    /// The typed sections as a serializable view (no `dirs`/`raw`/
    /// `templates`). Internal use only; public serialization of `Config`
    /// is the redacted tree.
    fn sections(&self) -> ConfigSections<'_> {
        ConfigSections {
            server: &self.server,
            search: &self.search,
            admission: &self.admission,
            health: &self.health,
            cache: &self.cache,
            merge: &self.merge,
            logs: &self.logs,
            ai: &self.ai,
            archive: &self.archive,
            auth: &self.auth,
            engines: &self.engines,
            config: &self.config,
        }
    }

    /// `load` against an explicit env map (tests; also the reason loads are
    /// deterministic under parallel test threads).
    fn load_with(env: &EnvMap) -> Result<Self, ConfigError> {
        let dirs = Dirs::detect_with(env);
        let path = dirs.config_file();

        // File layer. `raw` is what `save` writes back: the file verbatim,
        // or the built-in defaults tree (with its `${...}` templates intact)
        // when no file exists, so a first save produces a complete file.
        let (file, file_found): (toml::Value, bool) = match std::fs::read_to_string(&path) {
            Ok(text) => (
                toml::from_str(&text).map_err(|source| ConfigError::Parse {
                    path: path.clone(),
                    source,
                })?,
                true,
            ),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                (toml::Value::Table(toml::Table::new()), false)
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        let raw = if file_found {
            file.clone()
        } else {
            default_tree()?
        };

        // Validate the file layer (an empty table when no file exists); the
        // saved raw tree is the file contents or the defaults template tree.
        let mut cfg = Self::from_raw(&file, env)?;
        cfg.dirs = dirs;
        cfg.raw = Some(raw);
        Ok(cfg)
    }

    /// Validate and resolve a raw file-layer tree against an environment map.
    ///
    /// This is the in-memory path `Config::load` uses: env overrides,
    /// `${...}` interpolation, typed-section validation, built-in engine
    /// injection and `CAUCE_ENGINES` pinning. The returned `Config` has `raw`
    /// set to the submitted tree (so `save` preserves templates verbatim)
    /// and `dirs` resolved from `env`.
    pub fn from_raw(raw: &toml::Value, env: &EnvMap) -> Result<Self, ConfigError> {
        let dirs = Dirs::detect_with(env);

        // Resolution base is file + env only, never the defaults: serde
        // defaults are applied at `try_into` below, after interpolation, so
        // a `${...}` template in a `default = ...` attribute would stay a
        // harmless literal rather than failing on an unset variable.
        let mut merged = raw.clone();
        for (name, key_path, parse) in ENV_OVERRIDES {
            if let Some(value) = env.get(*name) {
                let value = if *parse {
                    env_scalar(value)
                } else {
                    toml::Value::String(value.clone())
                };
                set_path(&mut merged, key_path, value);
            }
        }

        // Interpolation switch is itself subject to file < env.
        let interpolate = merged
            .get("config")
            .and_then(|c| c.get("interpolation"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(true);

        let mut templates = BTreeMap::new();
        if interpolate {
            let mut at = Vec::new();
            interpolate_tree(&mut merged, env, &mut at, &mut templates)?;
        }

        let mut cfg: Config = merged.try_into().map_err(ConfigError::Invalid)?;

        if let Some(public_url) = &cfg.server.public_url {
            validate_public_url(public_url).map_err(|msg| ConfigError::InvalidValue {
                path: "server.public_url".to_string(),
                msg,
            })?;
        }

        // `cache.lexical.threshold` gates a serve decision: reject
        // non-finite and out-of-`(0.0, 1.0]` values rather than silently
        // voiding the Jaccard gate (`nan` would make `score < threshold`
        // always false, serving any same-params FTS candidate).
        let threshold = cfg.cache.lexical.threshold;
        if !threshold.is_finite() || threshold <= 0.0 || threshold > 1.0 {
            return Err(ConfigError::InvalidValue {
                path: "cache.lexical.threshold".to_string(),
                msg: format!("expected a finite value in (0.0, 1.0], got {threshold}"),
            });
        }

        // `merge.rrf_k` is the RRF denominator offset: a negative `k`
        // zeroes the denominator at `rank == -k - 1` and a non-finite one
        // poisons every score, so reject both rather than merge NaNs.
        let rrf_k = cfg.merge.rrf_k;
        if !rrf_k.is_finite() || rrf_k < 0.0 {
            return Err(ConfigError::InvalidValue {
                path: "merge.rrf_k".to_string(),
                msg: format!("expected a finite value >= 0.0, got {rrf_k}"),
            });
        }

        // `health.degraded_threshold` counts a streak: `0` would trip the
        // breaker on the first `Parse`/`Transport` error rather than after
        // a run of them, so it is rejected rather than re-interpreted.
        if cfg.health.degraded_threshold == 0 {
            return Err(ConfigError::InvalidValue {
                path: "health.degraded_threshold".to_string(),
                msg: "expected >= 1".to_string(),
            });
        }

        // The pair bounds the hedge point; an inverted window would fire
        // `Duration::clamp` outside its contract, so reject rather than
        // reorder silently.
        if cfg.search.hedge_floor_ms > cfg.search.hedge_ceiling_ms {
            return Err(ConfigError::InvalidValue {
                path: "search.hedge_floor_ms".to_string(),
                msg: format!(
                    "hedge_floor_ms ({}) must be <= hedge_ceiling_ms ({})",
                    cfg.search.hedge_floor_ms, cfg.search.hedge_ceiling_ms
                ),
            });
        }

        // Built-ins fill in entries the file did not define.
        for builtin in builtin_engines() {
            if !cfg.engines.iter().any(|e| e.id == builtin.id) {
                cfg.engines.push(builtin);
            }
        }
        for entry in &cfg.engines {
            // Ids surface in URL path segments, `fe-*` element ids, log
            // keys and fixture paths; outside `ENGINE_ID_PATTERN` they
            // collide or produce invalid markup downstream.
            if !EngineId::is_valid(entry.id.as_str()) {
                return Err(ConfigError::InvalidEngine {
                    id: entry.id.to_string(),
                    msg: format!("id must match {ENGINE_ID_PATTERN} (non-empty)"),
                });
            }
            if entry.kind == EngineKind::Exec && entry.command.is_none() {
                return Err(ConfigError::InvalidEngine {
                    id: entry.id.to_string(),
                    msg: "kind \"exec\" requires a command".to_string(),
                });
            }
            if let Some(egress) = &entry.egress
                && (egress.requests_per_second == 0 || egress.burst == 0)
            {
                return Err(ConfigError::InvalidEngine {
                    id: entry.id.to_string(),
                    msg: "egress.requests_per_second and egress.burst must be >= 1".to_string(),
                });
            }
        }

        // `CAUCE_ENGINES` pins the enabled set to exactly the listed ids.
        // An empty or whitespace-only value is treated as unset (documented
        // choice): pinning to nothing would produce a dead server.
        if let Some(list) = env.get("CAUCE_ENGINES").filter(|l| !l.trim().is_empty()) {
            let wanted: Vec<&str> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            for id in &wanted {
                if !cfg.engines.iter().any(|e| e.id.as_str() == *id) {
                    return Err(ConfigError::UnknownEngine((*id).to_string()));
                }
            }
            for entry in &mut cfg.engines {
                entry.enabled = wanted.contains(&entry.id.as_str());
            }
        }

        cfg.dirs = dirs;
        cfg.raw = Some(raw.clone());
        cfg.templates = templates;
        Ok(cfg)
    }

    /// `$CAUCE_CONFIG_DIR/config.toml`.
    pub fn config_path(&self) -> PathBuf {
        self.dirs.config_file()
    }

    /// Resolved config directory.
    pub fn config_dir(&self) -> &Path {
        &self.dirs.config_dir
    }

    /// Resolved data directory.
    pub fn data_dir(&self) -> &Path {
        &self.dirs.data_dir
    }

    /// `$CAUCE_DATA_DIR/cauce.db`.
    pub fn db_path(&self) -> PathBuf {
        self.dirs.db_path()
    }

    /// `$CAUCE_DATA_DIR/logs`.
    pub fn logs_dir(&self) -> PathBuf {
        self.dirs.logs_dir()
    }

    /// Create `config_dir`, `data_dir` and `logs_dir` if missing.
    pub fn ensure_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.config_dir())
    }

    /// Engines whose `enabled` flag is on (after `CAUCE_ENGINES` pinning).
    pub fn enabled_engines(&self) -> impl Iterator<Item = &EngineEntry> {
        self.engines.iter().filter(|e| e.enabled)
    }

    /// Look up one entry by id.
    pub fn engine(&self, id: &str) -> Option<&EngineEntry> {
        self.engines.iter().find(|e| e.id.as_str() == id)
    }

    /// The raw (pre-interpolation) file layer `save` writes back.
    pub fn raw_tree(&self) -> Option<&toml::Value> {
        self.raw.as_ref()
    }

    /// Write the raw template tree to `config_path`, creating the directory
    /// if needed. Resolved secrets are never written: values that came from
    /// `${env:...}`/`${file:...}` persist as their literal template text.
    ///
    /// The write is atomic (same-dir temp file + rename), so a crash mid-save
    /// cannot leave a truncated `config.toml`. Note for W0-09's
    /// `PUT /api/config`: the raw tree is re-serialized, so comments and
    /// formatting in the source file are not preserved; structure and
    /// templates are.
    pub fn save(&self) -> Result<PathBuf, ConfigError> {
        let path = self.config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| ConfigError::Write {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let raw = match &self.raw {
            Some(raw) => raw.clone(),
            None => default_tree()?,
        };
        let text = toml::to_string_pretty(&raw).map_err(ConfigError::Encode)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &text).map_err(|source| ConfigError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &path).map_err(|source| ConfigError::Write {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    /// The resolved config as a TOML tree with secrets redacted: every
    /// value that came from an interpolation template is shown as its raw
    /// `${...}` text instead of the resolved secret, and secret paths that
    /// were not template-produced (an `CAUCE_*` override or a file literal)
    /// render as `<redacted>`.
    pub fn display_tree(&self) -> Result<toml::Value, ConfigError> {
        let mut tree = to_value(&self.sections())?;
        for (path, raw) in &self.templates {
            set_display(
                tree_mut_at(&mut tree, path),
                toml::Value::String(raw.clone()),
            );
        }
        redact_secret_paths(&mut tree, &self.templates);
        Ok(tree)
    }

    /// `display_tree` rendered as TOML, for `cauce config show`.
    pub fn display_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(&self.display_tree()?).map_err(ConfigError::Encode)
    }

    /// Restore `<redacted>` placeholders in a `PUT /api/config` body from
    /// this config's secrets, before validation: `display_tree` renders
    /// secret leaves as `<redacted>`, so a show -> edit -> PUT roundtrip
    /// would otherwise write the literal string into `config.toml` and
    /// destroy the secret. Template-covered leaves get their raw `${...}`
    /// text back; literal secrets (file or `CAUCE_*` override) get the
    /// resolved value. Only paths `display_tree` can emit as `<redacted>`
    /// (`SECRET_PATHS` and `engines.<i>.env.<key>`, matched by engine id so
    /// a reordered array cannot leak one engine's secret onto another) are
    /// restored; every other placeholder — non-secret path, unknown path,
    /// unknown engine id, unset env key — is rejected so the literal can
    /// never be persisted. Returns the restored dotted paths.
    pub fn restore_redacted(
        &self,
        submitted: &mut toml::Value,
    ) -> Result<Vec<String>, ConfigError> {
        let mut leaves = Vec::new();
        redacted_leaves(submitted, &mut Vec::new(), &mut leaves);
        let mut restored = Vec::new();
        for path in leaves {
            let dotted = path.join(".");
            let replacement = self.redacted_source(submitted, &path).ok_or_else(|| {
                ConfigError::InvalidValue {
                    path: dotted.clone(),
                    msg: format!("{REDACTED} here has no current secret to restore"),
                }
            })?;
            if let Some(slot) = tree_mut_at(submitted, &path) {
                *slot = toml::Value::String(replacement);
                restored.push(dotted);
            }
        }
        Ok(restored)
    }

    /// The current value to write back for a `<redacted>` leaf at `path`:
    /// the raw `${...}` template text when this config has one at the same
    /// logical spot, otherwise the resolved value.
    fn redacted_source(&self, submitted: &toml::Value, path: &[String]) -> Option<String> {
        // `engines.<i>.env.<key>`: match the engine by id, not array index —
        // the submitted array may reorder entries.
        if path.len() == 4 && path[0] == "engines" && path[2] == "env" {
            // The index itself only needs to parse; the engine match is by id.
            path[1].parse::<usize>().ok()?;
            let id = tree_at(submitted, &path[..2])?.get("id")?.as_str()?;
            let cur = self.engines.iter().position(|e| e.id == id.into())?;
            let cur_path = vec![
                "engines".to_string(),
                cur.to_string(),
                "env".to_string(),
                path[3].clone(),
            ];
            if let Some(raw) = self.templates.get(&cur_path) {
                return Some(raw.clone());
            }
            return self.engines[cur].env.get(&path[3]).cloned();
        }
        // Anything outside SECRET_PATHS is not a path `display_tree` can
        // emit as `<redacted>`, so the placeholder is a fabrication: reject
        // it rather than guess at a source.
        if !SECRET_PATHS.iter().any(|p| {
            *p == path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
        }) {
            return None;
        }
        if let Some(raw) = self.templates.get(path) {
            return Some(raw.clone());
        }
        let resolved = to_value(&self.sections()).ok()?;
        tree_at(&resolved, path)?.as_str().map(String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{env_of, sandbox, write_config};
    use super::*;

    #[test]
    fn defaults_when_no_file() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.server.port, 4479);
        assert_eq!(cfg.search.deadline_ms, 3000);
        assert_eq!(cfg.search.min_results, 5);
        assert_eq!(cfg.search.ttl_s, 3600);
        assert_eq!(cfg.search.ttl_cap_s, 86400);
        assert_eq!(cfg.admission.max_wait_ms, 1500);
        assert_eq!(cfg.admission.max_concurrent_per_engine, 3);
        assert!(cfg.cache.lexical.enabled);
        assert_eq!(cfg.cache.lexical.threshold, 0.8);
        assert_eq!(cfg.logs.retention_days, 7);
        assert_eq!(cfg.ai.base_url, "");
        assert_eq!(cfg.ai.api_key, "");
        assert!(!cfg.ai.enabled);
        assert!(!cfg.auth.enabled);
        assert!(cfg.config.interpolation);
    }

    /// W1-13: the loopback classification shared by the `serve` bind
    /// refusal and the server's Host/Origin guard. Loopback means
    /// `localhost`, any `*.localhost` portless alias, `127.0.0.0/8` and
    /// `::1`, in bare or `host:port`/`[v6]:port` authority form.
    #[test]
    fn loopback_host_classification() {
        for ok in [
            "localhost",
            "LOCALHOST",
            "localhost.",
            "localhost:4479",
            "cauce.localhost",
            "search.localhost:443",
            "127.0.0.1",
            "127.0.0.1:4479",
            "127.53.0.9",
            "::1",
            "[::1]",
            "[::1]:4479",
        ] {
            assert!(is_loopback_host(ok), "{ok} must be loopback");
        }
        for no in [
            "0.0.0.0",
            "0.0.0.0:4479",
            "::",
            "[::]:4479",
            "192.168.1.10",
            "10.0.0.5",
            "example.com",
            "localhost.evil.com",
            "evil-localhost.com",
            "notlocalhost",
            "",
        ] {
            assert!(!is_loopback_host(no), "{no} must not be loopback");
        }
    }

    /// `auth.enabled` parses from `[auth]`; `enabled_for` forces it when
    /// the bind is not loopback (the former W6-03 line).
    #[test]
    fn auth_enabled_forces_on_non_loopback() {
        let (_tmp, env) = sandbox(&[]);
        write_config(
            Path::new(env.get("CAUCE_CONFIG_DIR").unwrap()),
            "[auth]\nenabled = true\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert!(cfg.auth.enabled);

        let off = AuthConfig::default();
        assert!(!off.enabled_for("127.0.0.1"));
        assert!(!off.enabled_for("localhost"));
        assert!(off.enabled_for("0.0.0.0"));
        assert!(off.enabled_for("192.168.1.10"));
        assert!(AuthConfig { enabled: true }.enabled_for("127.0.0.1"));
    }

    /// `CAUCE_*` dirs win over `XDG_*_HOME`, which wins over the home
    /// fallback; the resolved `Dirs` drive every path accessor on `Config`.
    #[test]
    fn dirs_cases() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.config_path(), cfg.config_dir().join("config.toml"));
        assert_eq!(cfg.db_path(), cfg.data_dir().join("cauce.db"));
        assert_eq!(cfg.logs_dir(), cfg.data_dir().join("logs"));

        struct DirsCase {
            env: &'static [(&'static str, &'static str)],
            config_dir: &'static str,
            data_dir: &'static str,
        }
        let cases = &[
            DirsCase {
                env: &[
                    ("XDG_CONFIG_HOME", "/tmp/xdgcfg"),
                    ("XDG_DATA_HOME", "/tmp/xdgdata"),
                ],
                config_dir: "/tmp/xdgcfg/cauce",
                data_dir: "/tmp/xdgdata/cauce",
            },
            DirsCase {
                env: &[("HOME", "/tmp/home")],
                config_dir: "/tmp/home/.config/cauce",
                data_dir: "/tmp/home/.local/share/cauce",
            },
            // `CAUCE_*` beats both lower precedence sources.
            DirsCase {
                env: &[
                    ("CAUCE_CONFIG_DIR", "/tmp/cc"),
                    ("CAUCE_DATA_DIR", "/tmp/cd"),
                    ("XDG_CONFIG_HOME", "/tmp/xdgcfg"),
                    ("XDG_DATA_HOME", "/tmp/xdgdata"),
                    ("HOME", "/tmp/home"),
                ],
                config_dir: "/tmp/cc",
                data_dir: "/tmp/cd",
            },
        ];
        for case in cases {
            let dirs = Dirs::detect_with(&env_of(case.env));
            assert_eq!(dirs.config_dir, PathBuf::from(case.config_dir));
            assert_eq!(dirs.data_dir, PathBuf::from(case.data_dir));
        }
    }

    /// `public_url` is optional; accepted values must be bare HTTP(S)
    /// origins and become the `public_origin` verbatim.
    #[test]
    fn public_url_cases() {
        #[derive(Debug)]
        enum PubUrlWant {
            Ok,
            Reject,
        }
        struct PubUrlCase {
            value: &'static str,
            want: PubUrlWant,
        }
        let cases = &[
            PubUrlCase {
                value: "https://search.localhost",
                want: PubUrlWant::Ok,
            },
            PubUrlCase {
                value: "https://search.localhost/",
                want: PubUrlWant::Ok,
            },
            PubUrlCase {
                value: "http://127.0.0.1:4480",
                want: PubUrlWant::Ok,
            },
            PubUrlCase {
                value: "relative/path",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "ftp://search.localhost",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://user:pass@search.localhost",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost/path",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost/?q=x",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost/#fragment",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost\" x=\"bad.localhost",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost\n.evil",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://bad host.localhost",
                want: PubUrlWant::Reject,
            },
            PubUrlCase {
                value: "https://search.localhost:invalid",
                want: PubUrlWant::Reject,
            },
        ];

        let (tmp, env) = sandbox(&[]);
        assert_eq!(Config::load_with(&env).unwrap().server.public_url, None);
        for case in cases {
            write_config(
                &tmp.path().join("cfg"),
                &format!("[server]\npublic_url = {:?}\n", case.value),
            );
            match (&case.want, Config::load_with(&env)) {
                (PubUrlWant::Ok, Ok(cfg)) => {
                    assert_eq!(cfg.server.public_url.as_deref(), Some(case.value));
                    assert_eq!(
                        cfg.server.public_origin("127.0.0.1", 4479),
                        case.value.trim_end_matches('/')
                    );
                }
                (PubUrlWant::Reject, Err(ConfigError::InvalidValue { ref path, .. })) => {
                    assert_eq!(path, "server.public_url");
                }
                (want, got) => panic!(
                    "public_url {:?}: expected {want:?}, got {got:?}",
                    case.value
                ),
            }
        }
    }

    #[test]
    fn public_origin_fallback_uses_bind_host_and_port_safely() {
        let server = ServerConfig::default();
        assert_eq!(
            server.public_origin("search.localhost", 4480),
            "http://search.localhost:4480"
        );
        assert_eq!(server.public_origin("::1", 4479), "http://[::1]:4479");
    }

    #[test]
    fn file_overrides_defaults() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[server]\nport = 4480\n[search]\ndeadline_ms = 1500\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.server.port, 4480);
        assert_eq!(cfg.search.deadline_ms, 1500);
        // Untouched fields keep their defaults.
        assert_eq!(cfg.server.host, "127.0.0.1");
        assert_eq!(cfg.search.min_results, 5);
    }

    #[test]
    fn cache_lexical_section_loads() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[cache.lexical]\nenabled = false\nthreshold = 0.5\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert!(!cfg.cache.lexical.enabled);
        assert_eq!(cfg.cache.lexical.threshold, 0.5);

        // A partial section fills the rest from defaults, and the section
        // renders in the displayed/default tree.
        write_config(
            &tmp.path().join("cfg"),
            "[cache.lexical]\nthreshold = 0.9\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert!(cfg.cache.lexical.enabled);
        assert_eq!(cfg.cache.lexical.threshold, 0.9);
        assert!(cfg.display_toml().unwrap().contains("lexical"));

        // Unknown keys inside the section are still rejected.
        write_config(&tmp.path().join("cfg"), "[cache.lexical]\nbogus = 1\n");
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::Invalid(_))
        ));
    }

    /// `threshold` gates a serve decision: non-finite and out-of-
    /// `(0.0, 1.0]` values are rejected at load rather than silently
    /// widening tier 2 (`nan` would accept every same-params candidate).
    #[test]
    fn lexical_threshold_out_of_range_is_rejected() {
        let (tmp, env) = sandbox(&[]);
        for value in ["nan", "-nan", "inf", "0.0", "-0.5", "1.5"] {
            write_config(
                &tmp.path().join("cfg"),
                &format!("[cache.lexical]\nthreshold = {value}\n"),
            );
            assert!(
                matches!(
                    Config::load_with(&env),
                    Err(ConfigError::InvalidValue { .. })
                ),
                "threshold = {value} must be rejected"
            );
        }
        // Boundaries: 1.0 is the inclusive upper bound.
        write_config(
            &tmp.path().join("cfg"),
            "[cache.lexical]\nthreshold = 1.0\n",
        );
        assert_eq!(
            Config::load_with(&env).unwrap().cache.lexical.threshold,
            1.0
        );
    }

    /// `[health]` (W3-07): the degraded-breaker pair loads from file and
    /// env with the settled defaults (5 consecutive `Parse`/`Transport`
    /// errors open the breaker for 600 s).
    #[test]
    fn health_section_loads() {
        let (tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.health.degraded_threshold, 5);
        assert_eq!(cfg.health.degraded_window_s, 600);
        assert!(cfg.display_toml().unwrap().contains("health"));

        // File layer, partial section: untouched field keeps its default.
        write_config(
            &tmp.path().join("cfg"),
            "[health]\ndegraded_threshold = 8\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.health.degraded_threshold, 8);
        assert_eq!(cfg.health.degraded_window_s, 600);

        // `CAUCE_*` pins beat the file.
        let (_tmp2, env_override) = sandbox(&[
            ("CAUCE_HEALTH_DEGRADED_THRESHOLD", "2"),
            ("CAUCE_HEALTH_DEGRADED_WINDOW_S", "120"),
        ]);
        let cfg = Config::load_with(&env_override).unwrap();
        assert_eq!(cfg.health.degraded_threshold, 2);
        assert_eq!(cfg.health.degraded_window_s, 120);

        // Unknown keys inside the section are still rejected.
        write_config(&tmp.path().join("cfg"), "[health]\nbogus = 1\n");
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::Invalid(_))
        ));
    }

    /// `degraded_threshold = 0` is not a valid streak length — it would
    /// open the breaker on the first `Parse`/`Transport` error — so it is
    /// rejected from either source rather than re-interpreted.
    #[test]
    fn health_degraded_threshold_zero_is_rejected() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[health]\ndegraded_threshold = 0\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "health.degraded_threshold"
        ));

        let (_tmp2, env_override) = sandbox(&[("CAUCE_HEALTH_DEGRADED_THRESHOLD", "0")]);
        assert!(matches!(
            Config::load_with(&env_override),
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "health.degraded_threshold"
        ));
    }

    /// An inverted hedge window (floor > ceiling) violates
    /// `Duration::clamp`'s contract at the hedge point; reject it from
    /// either source rather than panicking per engine task at runtime.
    #[test]
    fn hedge_floor_above_ceiling_is_rejected() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[search]\nhedge_floor_ms = 2500\nhedge_ceiling_ms = 1500\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "search.hedge_floor_ms"
        ));

        let (_tmp2, env_override) = sandbox(&[("CAUCE_SEARCH_HEDGE_FLOOR_MS", "2500")]);
        assert!(matches!(
            Config::load_with(&env_override),
            Err(ConfigError::InvalidValue { ref path, .. }) if path == "search.hedge_floor_ms"
        ));

        // Equal bounds are a legal (degenerate) fixed hedge point.
        write_config(
            &tmp.path().join("cfg"),
            "[search]\nhedge_floor_ms = 500\nhedge_ceiling_ms = 500\n",
        );
        assert!(Config::load_with(&env).is_ok());
    }

    #[test]
    fn env_overrides_file_and_defaults() {
        let (tmp, env) = sandbox(&[
            ("CAUCE_SERVER_PORT", "4490"),
            ("CAUCE_SERVER_PUBLIC_URL", "https://search.localhost"),
            ("CAUCE_AI_ENABLED", "true"),
        ]);
        write_config(&tmp.path().join("cfg"), "[server]\nport = 4480\n");
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.server.port, 4490);
        assert_eq!(
            cfg.server.public_url.as_deref(),
            Some("https://search.localhost")
        );
        assert!(cfg.ai.enabled);
    }

    #[test]
    fn deny_unknown_fields_rejects_unknown_keys() {
        let (tmp, env) = sandbox(&[]);
        write_config(&tmp.path().join("cfg"), "bogus = 1\n");
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::Invalid(_))
        ));
        write_config(&tmp.path().join("cfg"), "[server]\nbogus = 1\n");
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn save_round_trip_preserves_template() {
        let (tmp, env) = sandbox(&[("PROVIDER_API_KEY", "sk-live-secret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:PROVIDER_API_KEY}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "sk-live-secret");

        cfg.save().unwrap();
        let written = std::fs::read_to_string(cfg.config_path()).unwrap();
        assert!(written.contains("${env:PROVIDER_API_KEY}"), "{written}");
        assert!(!written.contains("sk-live-secret"), "{written}");

        // The saved file loads back into the same resolved config.
        let reloaded = Config::load_with(&env).unwrap();
        assert_eq!(reloaded.ai.api_key, "sk-live-secret");
    }

    #[test]
    fn builtin_engines_always_exist() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        let replay = cfg.engine("replay").unwrap();
        assert_eq!(replay.kind, EngineKind::Replay);
        let ddgs = cfg.engine("ddgs").unwrap();
        assert_eq!(ddgs.kind, EngineKind::Exec);
        assert_eq!(ddgs.command.as_deref(), Some("python3"));
        assert_eq!(
            ddgs.args,
            vec!["sdk/python/cauce_engine_sdk/ddgs_auto.py".to_string()]
        );
    }

    #[test]
    fn file_engine_entries_merge_with_builtins() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"custom\"\nkind = \"exec\"\ncommand = \"/bin/custom\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert!(cfg.engine("custom").unwrap().enabled);
        assert!(cfg.engine("replay").is_some());
        assert!(cfg.engine("ddgs").is_some());
    }

    #[test]
    fn file_engine_entry_shadows_builtin() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"ddgs\"\nkind = \"exec\"\ncommand = \"/opt/ddgs\"\nenabled = false\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let ddgs = cfg.engine("ddgs").unwrap();
        assert_eq!(ddgs.command.as_deref(), Some("/opt/ddgs"));
        assert!(!ddgs.enabled);
        // One ddgs entry only: the file shadowed the built-in.
        assert_eq!(
            cfg.engines
                .iter()
                .filter(|e| e.id.as_str() == "ddgs")
                .count(),
            1
        );
    }

    #[test]
    fn engine_egress_subtable_parses() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"bing\"\nkind = \"declarative\"\n\n[engines.egress]\nproxy = \"socks5://127.0.0.1:1080\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let egress = cfg.engine("bing").unwrap().egress.as_ref().unwrap();
        assert_eq!(egress.proxy.as_deref(), Some("socks5://127.0.0.1:1080"));
        // Settled politeness defaults.
        assert_eq!(egress.requests_per_second, 1);
        assert_eq!(egress.burst, 3);
        // Entries without the table stay direct.
        assert!(cfg.engine("ddgs").unwrap().egress.is_none());
    }

    /// `[[engines]]` semantic rejects share one file->`InvalidEngine`
    /// shape; the error names the offending id.
    #[test]
    fn engine_reject_cases() {
        struct RejectCase {
            file: &'static str,
            want_id: &'static str,
        }
        let cases = &[
            // `kind = "exec"` without a `command`.
            RejectCase {
                file: "[[engines]]\nid = \"x\"\nkind = \"exec\"\n",
                want_id: "x",
            },
            // A zero egress token-bucket rate.
            RejectCase {
                file: "[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.egress]\nrequests_per_second = 0\n",
                want_id: "x",
            },
        ];
        for case in cases {
            let (tmp, env) = sandbox(&[]);
            write_config(&tmp.path().join("cfg"), case.file);
            match Config::load_with(&env) {
                Err(ConfigError::InvalidEngine { id, .. }) => {
                    assert_eq!(id, case.want_id);
                }
                other => panic!("{:?} must be rejected: {other:?}", case.file),
            }
        }
    }

    /// Engine ids are `[A-Za-z0-9._-]+`: they surface in URL path
    /// segments, `fe-*` element ids, log keys and fixture paths, so
    /// `from_raw` rejects anything outside the charset and names the
    /// offending id. The `a.b`/`a-b` pair stays legal — `fe_id` encodes
    /// them injectively.
    #[test]
    fn engine_id_charset_is_enforced() {
        let (tmp, env) = sandbox(&[]);
        for bad in ["a b", "a:b", "a/b", "ünïcode", ""] {
            write_config(
                &tmp.path().join("cfg"),
                &format!("[[engines]]\nid = {bad:?}\nkind = \"replay\"\n"),
            );
            match Config::load_with(&env) {
                Err(ConfigError::InvalidEngine { id, msg }) => {
                    assert_eq!(id, bad);
                    assert!(msg.contains(ENGINE_ID_PATTERN), "{msg}");
                }
                other => panic!("id {bad:?} must be rejected: {other:?}"),
            }
        }

        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"a.b\"\nkind = \"replay\"\n\n[[engines]]\nid = \"a-b\"\nkind = \"replay\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert!(cfg.engine("a.b").is_some());
        assert!(cfg.engine("a-b").is_some());
    }

    /// `CAUCE_ENGINES` pins the enabled set to exactly the listed ids; an
    /// empty or whitespace-only value is treated as unset (pinning to
    /// nothing would produce a dead server), and every name must resolve
    /// against the configured + built-in entries.
    #[test]
    fn cauce_engines_pin_cases() {
        #[derive(Debug)]
        enum PinWant {
            Enabled(&'static [&'static str]),
            Unknown(&'static str),
        }
        struct PinCase {
            /// `CAUCE_ENGINES` value.
            engines: &'static str,
            /// TOML file body; empty means no file.
            file: &'static str,
            want: PinWant,
        }
        let cases = &[
            PinCase {
                engines: "replay",
                file: "",
                want: PinWant::Enabled(&["replay"]),
            },
            PinCase {
                engines: "ddgs",
                file: "",
                want: PinWant::Enabled(&["ddgs"]),
            },
            PinCase {
                engines: "replay, ddgs",
                file: "",
                want: PinWant::Enabled(&["ddgs", "replay"]),
            },
            PinCase {
                engines: "",
                file: "",
                want: PinWant::Enabled(&["ddgs"]),
            },
            PinCase {
                engines: "   ",
                file: "",
                want: PinWant::Enabled(&["ddgs"]),
            },
            PinCase {
                engines: "nosuch",
                file: "",
                want: PinWant::Unknown("nosuch"),
            },
            // A pin survives file entries it does not name.
            PinCase {
                engines: "replay",
                file: "[[engines]]\nid = \"custom\"\nkind = \"exec\"\ncommand = \"/bin/custom\"\n",
                want: PinWant::Enabled(&["replay"]),
            },
        ];
        for case in cases {
            let (tmp, env) = sandbox(&[("CAUCE_ENGINES", case.engines)]);
            if !case.file.is_empty() {
                write_config(&tmp.path().join("cfg"), case.file);
            }
            match (&case.want, Config::load_with(&env)) {
                (PinWant::Enabled(want), Ok(cfg)) => {
                    let mut enabled: Vec<_> =
                        cfg.enabled_engines().map(|e| e.id.as_str()).collect();
                    enabled.sort();
                    assert_eq!(
                        enabled.as_slice(),
                        *want,
                        "CAUCE_ENGINES={:?}",
                        case.engines
                    );
                }
                (PinWant::Unknown(want), Err(ConfigError::UnknownEngine(id))) => {
                    assert_eq!(&id, want);
                }
                (want, got) => {
                    panic!(
                        "CAUCE_ENGINES={:?}: expected {want:?}, got {got:?}",
                        case.engines
                    )
                }
            }
        }
    }
}
