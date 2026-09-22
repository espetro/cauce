//! Typed configuration, XDG directories, `${...}` interpolation and
//! resource-adaptive defaults (`Resources`).
//!
//! Settled contract (`.agents/plans/v3/wave-0-skeleton.md`, "Settled inputs"):
//! TOML at `$OXE_CONFIG_DIR/config.toml` (default `~/.config/oxe/`), data at
//! `$OXE_DATA_DIR` (default `~/.local/share/oxe/`: `oxe.db`, `logs/`).
//! Interpolation on string values at load: `${env:NAME}` (missing is an
//! error), `${env:NAME:-default}`, `${env:NAME:?msg}` (missing is an error
//! carrying `msg`), `${file:PATH}`, and `$$` as a literal `$`. The `:`
//! forms follow POSIX: unset-or-empty counts as missing. `Config::save`
//! writes the raw template tree back, never resolved secrets. Precedence
//! is defaults < file < `OXE_*` env.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::{EngineId, Tier};
use crate::store::StoreTuning;

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

/// `OXE_*` variables applied on top of the file layer (precedence env >
/// file > defaults). Each entry is `(env name, dotted TOML path, parse as
/// scalar)`; `parse` converts `true`/`false`/integers/floats before the
/// value is overlaid, the rest stay strings.
const ENV_OVERRIDES: &[(&str, &[&str], bool)] = &[
    ("OXE_SERVER_HOST", &["server", "host"], false),
    ("OXE_SERVER_PORT", &["server", "port"], true),
    ("OXE_SEARCH_DEADLINE_MS", &["search", "deadline_ms"], true),
    ("OXE_SEARCH_MIN_RESULTS", &["search", "min_results"], true),
    ("OXE_SEARCH_TTL_S", &["search", "ttl_s"], true),
    ("OXE_SEARCH_TTL_CAP_S", &["search", "ttl_cap_s"], true),
    (
        "OXE_ADMISSION_MAX_WAIT_MS",
        &["admission", "max_wait_ms"],
        true,
    ),
    (
        "OXE_ADMISSION_MAX_CONCURRENT_PER_ENGINE",
        &["admission", "max_concurrent_per_engine"],
        true,
    ),
    ("OXE_LOGS_RETENTION_DAYS", &["logs", "retention_days"], true),
    ("OXE_AI_BASE_URL", &["ai", "base_url"], false),
    ("OXE_AI_API_KEY", &["ai", "api_key"], false),
    ("OXE_AI_ENABLED", &["ai", "enabled"], true),
    (
        "OXE_CONFIG_INTERPOLATION",
        &["config", "interpolation"],
        true,
    ),
];

// Reserved `OXE_*` variables that are not config overrides: they steer
// directories (`OXE_CONFIG_DIR`, `OXE_DATA_DIR`), the enabled engine set
// (`OXE_ENGINES`) or other subsystems (`OXE_REPLAY_*`, `OXE_LOG_PRETTY`,
// `OXE_LIVE`, `OXE_NIGHTLY`).

/// What a secret leaf renders as in the display tree when it was not
/// produced by an interpolation template (templates print as their raw
/// `${...}` text instead).
const REDACTED: &str = "<redacted>";

/// Dotted paths whose values are secrets by position, so the display tree
/// redacts them no matter the value's origin (`${env:...}` template, `OXE_*`
/// override or a file literal). Engine `env` maps are covered separately:
/// every `engines.<i>.env.*` value is secret-bearing.
const SECRET_PATHS: &[&[&str]] = &[&["ai", "api_key"]];

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
    /// `OXE_ENGINES` named an engine with no configured or built-in entry.
    #[error("OXE_ENGINES names unknown engine {0:?}")]
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

/// Resolved filesystem locations. `OXE_CONFIG_DIR`/`OXE_DATA_DIR` win, then
/// `XDG_CONFIG_HOME`/`XDG_DATA_HOME`, then `~/.config/oxe` and
/// `~/.local/share/oxe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// Directory holding `config.toml`.
    pub config_dir: PathBuf,
    /// Data directory holding `oxe.db` and `logs/`.
    pub data_dir: PathBuf,
}

impl Dirs {
    /// Resolve from the process environment.
    pub fn detect() -> Self {
        Self::detect_with(&system_env())
    }

    fn detect_with(env: &EnvMap) -> Self {
        let config_dir = env
            .get("OXE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| env.get("XDG_CONFIG_HOME").map(|x| Path::new(x).join("oxe")))
            .unwrap_or_else(|| home_dir(env).join(".config/oxe"));
        let data_dir = env
            .get("OXE_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| env.get("XDG_DATA_HOME").map(|x| Path::new(x).join("oxe")))
            .unwrap_or_else(|| home_dir(env).join(".local/share/oxe"));
        Self {
            config_dir,
            data_dir,
        }
    }

    /// `$config_dir/config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// `$data_dir/oxe.db`.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("oxe.db")
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    4479
}

/// `[auth]`: the admin-auth switch (W1-13). The token mechanism itself is
/// deferred to `v3/later/postgres-and-multi-instance.md`; until it lands,
/// `enabled` is forced by the bind address — off on loopback, required off
/// it, so `oxe serve` refuses a non-loopback bind.
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

/// `[cache]`: cache-tier behaviour beyond TTLs (those live in `[search]`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// `[cache.lexical]`: the tier-2 FTS lookup (W1-10).
    #[serde(default)]
    pub lexical: LexicalConfig,
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

/// `[logs]`: JSONL log retention (W0-05 consumes `retention_days`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsConfig {
    /// Days a `logs/oxe-YYYY-MM-DD.jsonl` file is kept.
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
    /// Master switch; `false` until W4.
    #[serde(default)]
    pub enabled: bool,
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
    /// Member of the default fan-out set unless `OXE_ENGINES` overrides.
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
        },
        EngineEntry {
            id: EngineId::new("ddgs"),
            kind: EngineKind::Exec,
            enabled: true,
            command: Some("python3".to_string()),
            args: vec!["sdk/python/oxe_engine_sdk/ddgs_auto.py".to_string()],
            cwd: None,
            spec: None,
            tier: Some(Tier::T2),
            page_size: Some(10),
            egress: None,
            env: BTreeMap::new(),
        },
    ]
}

/// The resolved configuration: defaults < TOML file < `OXE_*` env.
///
/// Besides the typed sections it keeps the raw (pre-interpolation) file
/// tree for `save` and a map of template paths for redacted display; none
/// of that is part of the TOML schema (`#[serde(skip)]`).
///
/// `Debug` and `Serialize` are manual: both emit the *redacted* view
/// (`display_tree`), so a resolved secret (e.g. `ai.api_key` from
/// `${env:BIFROST_API_KEY}`) can never leak through `format!("{cfg:?}")`,
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
    /// `[cache]` section.
    #[serde(default)]
    pub cache: CacheConfig,
    /// `[logs]` section.
    #[serde(default)]
    pub logs: LogsConfig,
    /// `[ai]` section.
    #[serde(default)]
    pub ai: AiConfig,
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
    cache: &'a CacheConfig,
    logs: &'a LogsConfig,
    ai: &'a AiConfig,
    auth: &'a AuthConfig,
    engines: &'a [EngineEntry],
    config: &'a MetaConfig,
}

impl fmt::Debug for Config {
    /// Prints the redacted TOML, same as `oxe config show`.
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
            cache: CacheConfig::default(),
            logs: LogsConfig::default(),
            ai: AiConfig::default(),
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
    /// Load with the precedence chain defaults < `$OXE_CONFIG_DIR/config.toml`
    /// < `OXE_*` env. A missing file is not an error; a malformed or
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
            cache: &self.cache,
            logs: &self.logs,
            ai: &self.ai,
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
    /// injection and `OXE_ENGINES` pinning. The returned `Config` has `raw`
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

        // Built-ins fill in entries the file did not define.
        for builtin in builtin_engines() {
            if !cfg.engines.iter().any(|e| e.id == builtin.id) {
                cfg.engines.push(builtin);
            }
        }
        for entry in &cfg.engines {
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

        // `OXE_ENGINES` pins the enabled set to exactly the listed ids.
        // An empty or whitespace-only value is treated as unset (documented
        // choice): pinning to nothing would produce a dead server.
        if let Some(list) = env.get("OXE_ENGINES").filter(|l| !l.trim().is_empty()) {
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

    /// `$OXE_CONFIG_DIR/config.toml`.
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

    /// `$OXE_DATA_DIR/oxe.db`.
    pub fn db_path(&self) -> PathBuf {
        self.dirs.db_path()
    }

    /// `$OXE_DATA_DIR/logs`.
    pub fn logs_dir(&self) -> PathBuf {
        self.dirs.logs_dir()
    }

    /// Create `config_dir`, `data_dir` and `logs_dir` if missing.
    pub fn ensure_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.config_dir())
    }

    /// Engines whose `enabled` flag is on (after `OXE_ENGINES` pinning).
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
    /// were not template-produced (an `OXE_*` override or a file literal)
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

    /// `display_tree` rendered as TOML, for `oxe config show`.
    pub fn display_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(&self.display_tree()?).map_err(ConfigError::Encode)
    }

    /// Restore `<redacted>` placeholders in a `PUT /api/config` body from
    /// this config's secrets, before validation: `display_tree` renders
    /// secret leaves as `<redacted>`, so a show -> edit -> PUT roundtrip
    /// would otherwise write the literal string into `config.toml` and
    /// destroy the secret. Template-covered leaves get their raw `${...}`
    /// text back; literal secrets (file or `OXE_*` override) get the
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

/// Serialise then reparse to get a `toml::Value` view of `v` (there is no
/// direct `Serialize -> Value` path in the `toml` crate).
fn to_value<T: Serialize>(v: &T) -> Result<toml::Value, ConfigError> {
    let text = toml::to_string_pretty(v).map_err(ConfigError::Encode)?;
    toml::from_str(&text).map_err(ConfigError::Invalid)
}

/// The built-in defaults as a TOML tree; used as the raw layer when no
/// config file exists so a first `save` writes a complete template file.
fn default_tree() -> Result<toml::Value, ConfigError> {
    to_value(&Config::default().sections())
}

/// Navigate `tree` along `path` (numeric segments index into arrays) and
/// return the leaf slot, or `None` when the path does not resolve.
fn tree_mut_at<'a>(tree: &'a mut toml::Value, path: &[String]) -> Option<&'a mut toml::Value> {
    let mut cur = tree;
    for seg in path {
        cur = match cur {
            toml::Value::Table(t) => t.get_mut(seg)?,
            toml::Value::Array(a) => a.get_mut(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn set_display(slot: Option<&mut toml::Value>, value: toml::Value) {
    if let Some(slot) = slot {
        *slot = value;
    }
}

/// Replace the leaf at `path` with `<redacted>` unless `templates` covers
/// that path (a `${...}` template already displays as its raw text, which
/// reveals the indirection but never the secret) or the leaf is an empty
/// string (no secret to hide; showing `<redacted>` would falsely imply one
/// is set).
fn redact_leaf(tree: &mut toml::Value, path: &[String], templates: &BTreeMap<Vec<String>, String>) {
    if templates.contains_key(path) {
        return;
    }
    let Some(slot) = tree_mut_at(tree, path) else {
        return;
    };
    if let toml::Value::String(s) = &*slot
        && !s.is_empty()
    {
        *slot = toml::Value::String(REDACTED.to_string());
    }
}

/// Redact every secret-bearing leaf the template overlay did not already
/// cover: the fixed `SECRET_PATHS` plus every `engines.<i>.env.*` value
/// (child-process env vars are where engine credentials live). This is the
/// guard for secrets that entered the resolved config as literals — an
/// `OXE_*` override such as `OXE_AI_API_KEY` or a plain string in the file.
fn redact_secret_paths(tree: &mut toml::Value, templates: &BTreeMap<Vec<String>, String>) {
    for path in SECRET_PATHS {
        let owned: Vec<String> = path.iter().map(|s| (*s).to_string()).collect();
        redact_leaf(tree, &owned, templates);
    }
    // `engines` is a `&mut` borrow of `tree`, so the env leaves are
    // redacted in place rather than via `redact_leaf`/`tree_mut_at`.
    let Some(toml::Value::Array(engines)) = tree.get_mut("engines") else {
        return;
    };
    for (i, entry) in engines.iter_mut().enumerate() {
        let Some(toml::Value::Table(env)) = entry.get_mut("env") else {
            continue;
        };
        for (key, value) in env.iter_mut() {
            let path = vec![
                "engines".to_string(),
                i.to_string(),
                "env".to_string(),
                key.clone(),
            ];
            if templates.contains_key(&path) {
                continue;
            }
            if let toml::Value::String(s) = value
                && !s.is_empty()
            {
                *value = toml::Value::String(REDACTED.to_string());
            }
        }
    }
}

/// Navigate `tree` along `path` read-only; sibling of [`tree_mut_at`].
fn tree_at<'a>(tree: &'a toml::Value, path: &[String]) -> Option<&'a toml::Value> {
    let mut cur = tree;
    for seg in path {
        cur = match cur {
            toml::Value::Table(t) => t.get(seg)?,
            toml::Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Recursively collect the paths of every `<redacted>` string leaf.
fn redacted_leaves(tree: &toml::Value, at: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match tree {
        toml::Value::String(s) if s == REDACTED => out.push(at.clone()),
        toml::Value::Table(t) => {
            for (k, v) in t {
                at.push(k.clone());
                redacted_leaves(v, at, out);
                at.pop();
            }
        }
        toml::Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                at.push(i.to_string());
                redacted_leaves(v, at, out);
                at.pop();
            }
        }
        _ => {}
    }
}

/// Parse an `OXE_*` override string into a TOML scalar: booleans, integers
/// and floats get their native type; everything else stays a string.
fn env_scalar(s: &str) -> toml::Value {
    match s {
        "true" => return toml::Value::Boolean(true),
        "false" => return toml::Value::Boolean(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
    {
        return toml::Value::Float(f);
    }
    toml::Value::String(s.to_string())
}

/// Set `path` (e.g. `["server", "port"]`) inside a TOML tree, creating or
/// overwriting intermediate tables as needed.
fn set_path(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut cur = root;
    for seg in parents {
        if !cur.is_table() {
            *cur = toml::Value::Table(toml::Table::new());
        }
        let table = match cur.as_table_mut() {
            Some(t) => t,
            None => unreachable!("just converted to table"),
        };
        cur = table
            .entry((*seg).to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    }
    if !cur.is_table() {
        *cur = toml::Value::Table(toml::Table::new());
    }
    if let Some(table) = cur.as_table_mut() {
        table.insert((*last).to_string(), value);
    }
}

/// Recursively interpolate every string in the merged tree. `at` tracks the
/// dotted path for error messages; paths whose value changed are recorded
/// in `templates` (mapped to their raw text) for redacted display.
fn interpolate_tree(
    value: &mut toml::Value,
    env: &EnvMap,
    at: &mut Vec<String>,
    templates: &mut BTreeMap<Vec<String>, String>,
) -> Result<(), ConfigError> {
    match value {
        toml::Value::String(s) => {
            if s.contains('$') {
                let raw = std::mem::take(s);
                *s = interpolate_str(&raw, env, &at.join("."))?;
                if *s != raw {
                    templates.insert(at.clone(), raw);
                }
            }
        }
        toml::Value::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                at.push(i.to_string());
                interpolate_tree(item, env, at, templates)?;
                at.pop();
            }
        }
        toml::Value::Table(table) => {
            for (key, item) in table.iter_mut() {
                at.push(key.clone());
                interpolate_tree(item, env, at, templates)?;
                at.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

/// Interpolate one string value: `${env:...}`, `${file:...}`, `$$` escape.
/// A bare `$` not followed by `$` or `{` is literal.
///
/// Public so engine spec loaders (oxe-engines `declarative`) can run the
/// same `${env:NAME}`/`${file:PATH}` contract on `request.headers` values.
/// `path` is the dotted location used in error messages.
pub fn interpolate_str(raw: &str, env: &EnvMap, path: &str) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(tail) = after.strip_prefix('$') {
            out.push('$');
            rest = tail;
        } else if let Some(tail) = after.strip_prefix('{') {
            let end = tail
                .find('}')
                .ok_or_else(|| ConfigError::BadInterpolation {
                    path: path.to_string(),
                    expr: format!("${{{tail}"),
                })?;
            out.push_str(&resolve_expr(&tail[..end], env, path)?);
            rest = &tail[end + 1..];
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// Resolve the inside of a `${...}` expression.
fn resolve_expr(inner: &str, env: &EnvMap, path: &str) -> Result<String, ConfigError> {
    if let Some(spec) = inner.strip_prefix("env:") {
        // First `:-` or `:?` wins; the rest of the string is the payload.
        let split = [(":-", '-'), (":?", '?')]
            .into_iter()
            .filter_map(|(sep, kind)| spec.find(sep).map(|i| (i, kind)))
            .min_by_key(|(i, _)| *i);
        let (name, suffix) = match split {
            Some((i, kind)) => (&spec[..i], Some((kind, &spec[i + 2..]))),
            None => (spec, None),
        };
        // POSIX semantics: the `:`-forms (`:-`, `:?`) treat unset OR empty
        // as missing; the plain form treats empty as a real value.
        let value = env.get(name).filter(|v| !v.is_empty());
        return match suffix {
            Some(('-', default)) => Ok(value.cloned().unwrap_or_else(|| default.to_string())),
            Some(('?', msg)) => value.cloned().ok_or_else(|| ConfigError::MissingEnvMsg {
                path: path.to_string(),
                var: name.to_string(),
                msg: msg.to_string(),
            }),
            // Plain `${env:NAME}`: unset errors, empty stays empty.
            _ => env
                .get(name)
                .cloned()
                .ok_or_else(|| ConfigError::MissingEnv {
                    path: path.to_string(),
                    var: name.to_string(),
                }),
        };
    }
    if let Some(file) = inner.strip_prefix("file:") {
        let file = expand_home(file, env);
        return std::fs::read_to_string(&file)
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|source| ConfigError::MissingFile {
                path: path.to_string(),
                file,
                source,
            });
    }
    Err(ConfigError::BadInterpolation {
        path: path.to_string(),
        expr: format!("${{{inner}}}"),
    })
}

/// `~/...` inside `${file:...}` resolves against the user's home directory.
fn expand_home(path: &str, env: &EnvMap) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home_dir(env).join(rest),
        None => PathBuf::from(path),
    }
}

/// Host resources detected at startup; the resource-adaptive defaults the
/// settled inputs require (never a fixed large allocation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resources {
    /// SQLite `cache_size`/`mmap_size`/`busy_timeout` tuning (W0-03 type,
    /// consumed by `oxe-store-sqlite`).
    pub store_tuning: StoreTuning,
    /// Max concurrent upstream engine calls, scaled to cores.
    pub upstream_concurrency: u16,
    /// `cargo test`/`nextest` thread budget, scaled to cores.
    pub test_threads: u16,
}

impl Resources {
    /// Read available memory and core count via `sysinfo`, then derive.
    pub fn detect() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();
        let mem_bytes = sys.available_memory();
        let cores = sysinfo::System::physical_core_count()
            .unwrap_or_else(|| sys.cpus().len())
            .min(usize::from(u16::MAX)) as u16;
        Self::from_specs(mem_bytes, cores)
    }

    /// Pure derivation from injected numbers (the acceptance test path).
    ///
    /// Scaling rules, clamped so small machines stay usable and big ones do
    /// not get an unbounded allocation:
    ///
    /// - `cache_size_kib`: RAM / 128, clamped to 8-512 MiB
    ///   (4 GiB -> 32 MiB, 32 GiB -> 256 MiB).
    /// - `mmap_size_bytes`: RAM / 16, clamped to 64 MiB-4 GiB
    ///   (4 GiB -> 256 MiB, 32 GiB -> 2 GiB).
    /// - `busy_timeout_ms`: fixed 5000.
    /// - `upstream_concurrency`: 4 per core, clamped to 4-64.
    /// - `test_threads`: one per core, clamped to 1-32.
    pub fn from_specs(mem_bytes: u64, cores: u16) -> Self {
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        const GIB: u64 = 1024 * MIB;
        let cache_size_kib = (mem_bytes / 128 / KIB).clamp(8 * KIB, 512 * KIB) as u32;
        let mmap_size_bytes = (mem_bytes / 16).clamp(64 * MIB, 4 * GIB);
        Self {
            store_tuning: StoreTuning {
                cache_size_kib,
                mmap_size_bytes,
                busy_timeout_ms: 5_000,
            },
            upstream_concurrency: (u32::from(cores) * 4).clamp(4, 64) as u16,
            test_threads: cores.clamp(1, 32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> EnvMap {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// Temp config + data dirs, returned with an env map pointing at them.
    fn sandbox(extra: &[(&str, &str)]) -> (tempfile::TempDir, EnvMap) {
        let dir = tempfile::tempdir().unwrap();
        let mut map = env_of(&[
            ("OXE_CONFIG_DIR", dir.path().join("cfg").to_str().unwrap()),
            ("OXE_DATA_DIR", dir.path().join("data").to_str().unwrap()),
        ]);
        for (k, v) in extra {
            map.insert((*k).to_string(), (*v).to_string());
        }
        (dir, map)
    }

    fn write_config(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("config.toml"), body).unwrap();
    }

    const GIB: u64 = 1024 * 1024 * 1024;

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
            "oxe.localhost",
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
            Path::new(env.get("OXE_CONFIG_DIR").unwrap()),
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

    #[test]
    fn dirs_resolve_oxe_then_xdg_then_home() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.config_path(), cfg.config_dir().join("config.toml"));
        assert_eq!(cfg.db_path(), cfg.data_dir().join("oxe.db"));
        assert_eq!(cfg.logs_dir(), cfg.data_dir().join("logs"));

        let xdg = env_of(&[
            ("XDG_CONFIG_HOME", "/tmp/xdgcfg"),
            ("XDG_DATA_HOME", "/tmp/xdgdata"),
        ]);
        let dirs = Dirs::detect_with(&xdg);
        assert_eq!(dirs.config_dir, PathBuf::from("/tmp/xdgcfg/oxe"));
        assert_eq!(dirs.data_dir, PathBuf::from("/tmp/xdgdata/oxe"));

        let home = env_of(&[("HOME", "/tmp/home")]);
        let dirs = Dirs::detect_with(&home);
        assert_eq!(dirs.config_dir, PathBuf::from("/tmp/home/.config/oxe"));
        assert_eq!(dirs.data_dir, PathBuf::from("/tmp/home/.local/share/oxe"));
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

    #[test]
    fn env_overrides_file_and_defaults() {
        let (tmp, env) = sandbox(&[("OXE_SERVER_PORT", "4490"), ("OXE_AI_ENABLED", "true")]);
        write_config(&tmp.path().join("cfg"), "[server]\nport = 4480\n");
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.server.port, 4490);
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
    fn interpolation_env_required() {
        let (tmp, env) = sandbox(&[("MY_SECRET", "s3cret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:MY_SECRET}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "s3cret");
    }

    #[test]
    fn interpolation_env_missing_fails() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:UNSET_VAR}\"\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::MissingEnv { var, .. }) if var == "UNSET_VAR"
        ));
    }

    #[test]
    fn interpolation_env_default() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:UNSET_VAR:-fallback}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "fallback");

        let (tmp, env) = sandbox(&[("SET_VAR", "real")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:SET_VAR:-fallback}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "real");
    }

    #[test]
    fn interpolation_env_required_message_fails_startup() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:UNSET_VAR:?get a Bifrost key first}\"\n",
        );
        match Config::load_with(&env) {
            Err(ConfigError::MissingEnvMsg { var, msg, .. }) => {
                assert_eq!(var, "UNSET_VAR");
                assert_eq!(msg, "get a Bifrost key first");
            }
            other => panic!("expected MissingEnvMsg, got {other:?}"),
        }
    }

    #[test]
    fn interpolation_file_reads_and_trims() {
        let (tmp, env) = sandbox(&[]);
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "file-secret\n").unwrap();
        write_config(
            &tmp.path().join("cfg"),
            &format!("[ai]\napi_key = \"${{file:{}}}\"\n", secret.display()),
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "file-secret");
    }

    #[test]
    fn interpolation_file_missing_fails() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${file:/nonexistent/secret}\"\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::MissingFile { .. })
        ));
    }

    #[test]
    fn interpolation_dollar_escape() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"literal $$HOME and $$\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "literal $HOME and $");
    }

    #[test]
    fn interpolation_empty_env_uses_default_posix() {
        // POSIX: `:-` and `:?` treat unset-or-empty as missing.
        let (tmp, env) = sandbox(&[("EMPTY_VAR", "")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:EMPTY_VAR:-fallback}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "fallback");

        // Plain `${env:NAME}` keeps the empty value.
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:EMPTY_VAR}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "");

        // `:?` on an empty var fails startup like an unset one.
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:EMPTY_VAR:?need a key}\"\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::MissingEnvMsg { .. })
        ));
    }

    #[test]
    fn interpolation_escaped_template_is_literal() {
        // `$${env:X}` produces the literal text `${env:X}`; no expansion.
        let (tmp, env) = sandbox(&[("MY_SECRET", "s3cret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"$${env:MY_SECRET}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "${env:MY_SECRET}");
    }

    #[test]
    fn interpolation_adjacent_templates() {
        let (tmp, env) = sandbox(&[("PART_A", "sk-"), ("PART_B", "bf-123")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:PART_A}${env:PART_B}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "sk-bf-123");
    }

    #[test]
    fn interpolation_required_message_with_set_var_uses_value() {
        let (tmp, env) = sandbox(&[("SET_VAR", "real-value")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:SET_VAR:?unreachable}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "real-value");
    }

    #[test]
    fn interpolation_disabled_keeps_literals() {
        let (tmp, env) = sandbox(&[("MY_SECRET", "s3cret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[config]\ninterpolation = false\n[ai]\napi_key = \"${env:MY_SECRET}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "${env:MY_SECRET}");
    }

    #[test]
    fn save_round_trip_preserves_template() {
        let (tmp, env) = sandbox(&[("BIFROST_API_KEY", "sk-bf-live-secret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:BIFROST_API_KEY}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "sk-bf-live-secret");

        cfg.save().unwrap();
        let written = std::fs::read_to_string(cfg.config_path()).unwrap();
        assert!(written.contains("${env:BIFROST_API_KEY}"), "{written}");
        assert!(!written.contains("sk-bf-live-secret"), "{written}");

        // The saved file loads back into the same resolved config.
        let reloaded = Config::load_with(&env).unwrap();
        assert_eq!(reloaded.ai.api_key, "sk-bf-live-secret");
    }

    #[test]
    fn display_redacts_resolved_secrets() {
        let (tmp, env) = sandbox(&[("BIFROST_API_KEY", "sk-bf-live-secret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:BIFROST_API_KEY}\"\n[server]\nport = 4480\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let shown = cfg.display_toml().unwrap();
        assert!(shown.contains("${env:BIFROST_API_KEY}"), "{shown}");
        assert!(!shown.contains("sk-bf-live-secret"), "{shown}");
        assert!(shown.contains("4480"), "{shown}");
    }

    #[test]
    fn debug_and_serialize_redact_secrets() {
        let (tmp, env) = sandbox(&[("BIFROST_API_KEY", "sk-bf-live-secret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"${env:BIFROST_API_KEY}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "sk-bf-live-secret");

        for rendered in [
            format!("{cfg:?}"),
            serde_json::to_string(&cfg).unwrap(),
            toml::to_string_pretty(&cfg).unwrap(),
        ] {
            assert!(!rendered.contains("sk-bf-live-secret"), "{rendered}");
            assert!(rendered.contains("${env:BIFROST_API_KEY}"), "{rendered}");
        }
    }

    /// An `OXE_*` override lands as a literal in the resolved config — no
    /// `${...}` template tracks it — yet secret paths are redacted by
    /// position on every display surface (#82).
    #[test]
    fn display_redacts_env_override_secret() {
        let (_tmp, env) = sandbox(&[("OXE_AI_API_KEY", "s3cret-from-env")]);
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "s3cret-from-env");

        for rendered in [
            cfg.display_toml().unwrap(),
            format!("{cfg:?}"),
            serde_json::to_string(&cfg).unwrap(),
            toml::to_string_pretty(&cfg).unwrap(),
        ] {
            assert!(!rendered.contains("s3cret-from-env"), "{rendered}");
            assert!(rendered.contains(REDACTED), "{rendered}");
        }
    }

    /// File literals at secret paths are redacted too, and every
    /// `engines.*.env.*` value is treated as secret-bearing — unless it
    /// came from a template, which keeps its raw `${...}` display text.
    #[test]
    fn display_redacts_literal_and_engine_env_secrets() {
        let (tmp, env) = sandbox(&[("ENGINE_TMPL", "tmpl-secret")]);
        write_config(
            &tmp.path().join("cfg"),
            "[ai]\napi_key = \"literal-secret\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"engine-secret\"\nOTHER = \"${env:ENGINE_TMPL}\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        assert_eq!(cfg.ai.api_key, "literal-secret");
        assert_eq!(
            cfg.engine("x")
                .unwrap()
                .env
                .get("MY_KEY")
                .map(String::as_str),
            Some("engine-secret")
        );

        let shown = cfg.display_toml().unwrap();
        for secret in ["literal-secret", "engine-secret", "tmpl-secret"] {
            assert!(!shown.contains(secret), "{shown}");
        }
        // The template-valued env entry still shows its `${...}` text.
        assert!(shown.contains("${env:ENGINE_TMPL}"), "{shown}");
    }

    /// Redacting an unset secret path must not fabricate one: an empty
    /// `api_key` displays as `""`, not `<redacted>`.
    #[test]
    fn display_does_not_redact_empty_secret() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        let shown = cfg.display_toml().unwrap();
        assert!(!shown.contains(REDACTED), "{shown}");
        assert!(shown.contains("api_key = \"\""), "{shown}");
    }

    /// A display -> PUT roundtrip restores `<redacted>` leaves to their
    /// real values (template text or resolved literal) instead of
    /// persisting the placeholder.
    #[test]
    fn restore_redacted_roundtrips_secret_leaves() {
        let (_tmp, env) = sandbox(&[("OXE_AI_API_KEY", "env-secret"), ("TMPL", "t-secret")]);
        let mut submitted: toml::Value = toml::from_str(
            "[ai]\napi_key = \"<redacted>\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"<redacted>\"\nOTHER = \"${env:TMPL}\"\n",
        )
        .unwrap();

        // No current secret behind the placeholders yet.
        let (_tmp2, env2) = sandbox(&[("TMPL", "t-secret")]);
        let empty = Config::load_with(&env2).unwrap();
        assert!(empty.restore_redacted(&mut submitted.clone()).is_err());

        // With a current config that has the secrets, both restore.
        write_config(
            &_tmp.path().join("cfg"),
            "[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"file-secret\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let restored = cfg.restore_redacted(&mut submitted).unwrap();
        assert_eq!(restored, ["ai.api_key", "engines.0.env.MY_KEY"]);
        assert_eq!(
            tree_at(&submitted, &["ai".into(), "api_key".into()]).and_then(|v| v.as_str()),
            Some("env-secret")
        );
        assert_eq!(
            tree_at(
                &submitted,
                &["engines".into(), "0".into(), "env".into(), "MY_KEY".into()]
            )
            .and_then(|v| v.as_str()),
            Some("file-secret")
        );
        // The restored tree validates and keeps the template untouched.
        let cfg2 = Config::from_raw(&submitted, &env).unwrap();
        assert_eq!(cfg2.ai.api_key, "env-secret");
        assert_eq!(
            cfg2.engine("x")
                .unwrap()
                .env
                .get("MY_KEY")
                .map(String::as_str),
            Some("file-secret")
        );
    }

    /// A `<redacted>` at a non-secret path (or any spot with no current
    /// secret) is rejected rather than persisted as a literal.
    #[test]
    fn restore_redacted_rejects_orphaned_placeholder() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        let mut submitted: toml::Value =
            toml::from_str("[search]\ndeadline_ms = \"<redacted>\"\n").unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());

        // String-typed non-secret paths are rejected too, and a fabricated
        // `<redacted>` on an engine field cannot leak a *different* engine's
        // value across a reordered array.
        let mut submitted: toml::Value = toml::from_str(
            "[server]\nhost = \"<redacted>\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"<redacted>\"\n",
        )
        .unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());

        // Unknown engine id under env is rejected.
        let mut submitted: toml::Value = toml::from_str(
            "[[engines]]\nid = \"ghost\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nK = \"<redacted>\"\n",
        )
        .unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());
    }

    /// Engine env placeholders bind by engine id, not array index: a
    /// submitted `[[engines]]` order different from the current config still
    /// restores each secret onto the right engine.
    #[test]
    fn restore_redacted_matches_engine_env_by_id() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"a\"\nkind = \"exec\"\ncommand = \"/bin/a\"\n\n[engines.env]\nK = \"secret-a\"\n\n[[engines]]\nid = \"b\"\nkind = \"exec\"\ncommand = \"/bin/b\"\n\n[engines.env]\nK = \"secret-b\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();

        // Submitted order is b, a — the reverse of the file.
        let mut submitted: toml::Value = toml::from_str(
            "[[engines]]\nid = \"b\"\nkind = \"exec\"\ncommand = \"/bin/b\"\n\n[engines.env]\nK = \"<redacted>\"\n\n[[engines]]\nid = \"a\"\nkind = \"exec\"\ncommand = \"/bin/a\"\n\n[engines.env]\nK = \"<redacted>\"\n",
        )
        .unwrap();
        let mut restored = cfg.restore_redacted(&mut submitted).unwrap();
        restored.sort();
        assert_eq!(restored, ["engines.0.env.K", "engines.1.env.K"]);
        let k = |i: &str| {
            tree_at(
                &submitted,
                &["engines".into(), i.into(), "env".into(), "K".into()],
            )
            .and_then(|v| v.as_str().map(String::from))
        };
        assert_eq!(k("0").as_deref(), Some("secret-b"));
        assert_eq!(k("1").as_deref(), Some("secret-a"));
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
            vec!["sdk/python/oxe_engine_sdk/ddgs_auto.py".to_string()]
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

    #[test]
    fn engine_egress_zero_rate_rejected() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.egress]\nrequests_per_second = 0\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::InvalidEngine { id, .. }) if id == "x"
        ));
    }

    #[test]
    fn exec_engine_requires_command() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"x\"\nkind = \"exec\"\n",
        );
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::InvalidEngine { .. })
        ));
    }

    #[test]
    fn oxe_engines_pins_enabled_set() {
        let (_tmp, env) = sandbox(&[("OXE_ENGINES", "replay")]);
        let cfg = Config::load_with(&env).unwrap();
        let enabled: Vec<_> = cfg.enabled_engines().map(|e| e.id.as_str()).collect();
        assert_eq!(enabled, ["replay"]);

        let (_tmp, env) = sandbox(&[("OXE_ENGINES", "ddgs")]);
        let cfg = Config::load_with(&env).unwrap();
        let enabled: Vec<_> = cfg.enabled_engines().map(|e| e.id.as_str()).collect();
        assert_eq!(enabled, ["ddgs"]);

        let (_tmp, env) = sandbox(&[("OXE_ENGINES", "replay, ddgs")]);
        let cfg = Config::load_with(&env).unwrap();
        let mut enabled: Vec<_> = cfg.enabled_engines().map(|e| e.id.as_str()).collect();
        enabled.sort();
        assert_eq!(enabled, ["ddgs", "replay"]);
    }

    #[test]
    fn oxe_engines_empty_is_unset() {
        // `OXE_ENGINES=""` (or whitespace) is treated as unset: pinning to
        // zero engines would produce a dead server.
        for value in ["", "   "] {
            let (_tmp, env) = sandbox(&[("OXE_ENGINES", value)]);
            let cfg = Config::load_with(&env).unwrap();
            let enabled: Vec<_> = cfg.enabled_engines().map(|e| e.id.as_str()).collect();
            assert_eq!(enabled, ["ddgs"]);
        }
    }

    #[test]
    fn oxe_engines_unknown_id_fails() {
        let (_tmp, env) = sandbox(&[("OXE_ENGINES", "nosuch")]);
        assert!(matches!(
            Config::load_with(&env),
            Err(ConfigError::UnknownEngine(id)) if id == "nosuch"
        ));
    }

    #[test]
    fn oxe_engines_replay_works_when_file_lists_others() {
        let (tmp, env) = sandbox(&[("OXE_ENGINES", "replay")]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"custom\"\nkind = \"exec\"\ncommand = \"/bin/custom\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let enabled: Vec<_> = cfg.enabled_engines().map(|e| e.id.as_str()).collect();
        assert_eq!(enabled, ["replay"]);
    }

    #[test]
    fn resources_scale_with_memory() {
        let small = Resources::from_specs(4 * GIB, 8);
        let big = Resources::from_specs(32 * GIB, 8);
        assert!(small.store_tuning.cache_size_kib < big.store_tuning.cache_size_kib);
        assert!(small.store_tuning.mmap_size_bytes < big.store_tuning.mmap_size_bytes);
        assert_eq!(small.store_tuning.busy_timeout_ms, 5_000);
    }

    #[test]
    fn resources_clamp_extremes() {
        let tiny = Resources::from_specs(512 * 1024 * 1024, 1);
        assert_eq!(tiny.store_tuning.cache_size_kib, 8 * 1024);
        assert_eq!(tiny.test_threads, 1);
        assert_eq!(tiny.upstream_concurrency, 4);

        let huge = Resources::from_specs(1024 * GIB, 128);
        assert_eq!(huge.store_tuning.cache_size_kib, 512 * 1024);
        assert_eq!(huge.store_tuning.mmap_size_bytes, 4 * GIB);
        assert_eq!(huge.upstream_concurrency, 64);
        assert_eq!(huge.test_threads, 32);
    }
}
