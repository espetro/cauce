//! Observability foundation (parent plan 6.1, wave-0 step W0-05).
//!
//! - `init` installs a `tracing` subscriber with a JSONL file layer
//!   (`<data_dir>/logs/oxe-YYYY-MM-DD.jsonl`, daily rotation via
//!   `tracing-appender`, retention `retention_days`) and, when stderr is a
//!   TTY or `OXE_LOG_PRETTY=1`, a pretty stderr layer.
//! - With the `otlp` feature (non-default), OTLP trace export is wired but
//!   only activates when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.
//! - `RequestId` is a UUIDv7 carried as a `request_id` span field; the JSONL
//!   layer hoists it to a top-level field on every line.
//! - `audit` emits a JSONL event with `audit=true` and appends the `audit`
//!   table row through `Store::audit`, one call site per audited action.
//! - `trace` reads the JSONL files for `oxe trace <request_id>`; W2's
//!   `/trace/{id}` page reuses the same reader.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod audit;
mod jsonl;
#[cfg(feature = "otlp")]
mod otlp;
mod request;
pub mod trace;

use std::io::IsTerminal;
use std::path::PathBuf;

use oxe_core::config::Dirs;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

pub use audit::audit;
pub use request::{RequestId, request_span};

/// The oxe data directory. Delegates to `oxe_core::config::Dirs` so there
/// is exactly one resolver: `OXE_DATA_DIR` > `XDG_DATA_HOME` >
/// `~/.local/share/oxe`.
pub fn data_dir() -> PathBuf {
    Dirs::detect().data_dir
}

/// Directory holding the JSONL logs (`<data_dir>/logs`).
pub fn logs_dir() -> PathBuf {
    Dirs::detect().logs_dir()
}

/// Knobs for [`init`]. `Default` resolves the environment; callers that
/// already loaded config (W0-11) set the fields directly.
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// Directory for the daily JSONL files. Default: [`logs_dir`].
    pub logs_dir: PathBuf,
    /// Days of rotated files to keep (`logs.retention_days`). Default 7;
    /// `OXE_LOG_RETENTION_DAYS` overrides until config lands.
    pub retention_days: usize,
    /// Emit the pretty human layer on stderr. Default: `OXE_LOG_PRETTY=1`
    /// or stderr is a TTY.
    pub stderr_pretty: bool,
    /// `tracing` filter directive for the stderr and OTLP layers. Default:
    /// `OXE_LOG`, then `RUST_LOG`, then `info`. The JSONL layer is NOT
    /// filtered by this: it keeps a fixed `info` floor so `RUST_LOG=warn`
    /// cannot silently starve `oxe trace`.
    pub filter: String,
    /// Compile-time `otlp` feature is separate; this flag controls the
    /// runtime check of `OTEL_EXPORTER_OTLP_ENDPOINT`.
    pub otlp: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        let retention_days = std::env::var("OXE_LOG_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7);
        let stderr_pretty = std::env::var_os("OXE_LOG_PRETTY").is_some_and(|v| v == "1")
            || std::io::stderr().is_terminal();
        let filter = std::env::var("OXE_LOG")
            .ok()
            .or_else(|| std::env::var("RUST_LOG").ok())
            .unwrap_or_else(|| "info".to_string());
        Self {
            logs_dir: logs_dir(),
            retention_days,
            stderr_pretty,
            filter,
            otlp: true,
        }
    }
}

/// Keeps the pipeline alive: flushes the file writer on drop and shuts the
/// OTLP tracer provider down so buffered spans are exported.
///
/// Drop it on shutdown; on a tokio `current_thread` runtime the provider
/// shutdown runs on a helper thread to avoid the documented deadlock.
pub struct ObservabilityGuard {
    /// Kept for its `Drop`, which drains the writer queue.
    _writer_guard: tracing_appender::non_blocking::WorkerGuard,
    #[cfg(feature = "otlp")]
    otlp: Option<otlp::OtlpHandle>,
}

impl ObservabilityGuard {
    /// Flush pending lines and shut down OTLP export.
    pub fn shutdown(self) {
        drop(self);
    }
}

#[cfg(feature = "otlp")]
impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(otlp) = self.otlp.take() {
            otlp.shutdown();
        }
    }
}

/// Build the subscriber without installing it. Returns the dispatch plus the
/// guard that owns the writer/OTLP lifetimes. Used by tests; `init` calls it
/// and installs globally.
pub fn build(
    config: &ObservabilityConfig,
) -> Result<(tracing::Dispatch, ObservabilityGuard), jsonl::LogInitError> {
    // The env-controlled filter applies to the stderr and OTLP layers only.
    let filter = EnvFilter::try_new(&config.filter).unwrap_or_else(|e| {
        eprintln!(
            "oxe: invalid log filter {:?} ({e}); falling back to \"info\"",
            config.filter
        );
        EnvFilter::new("info")
    });

    let (writer, writer_guard) = jsonl::daily_file_writer(&config.logs_dir, config.retention_days)?;
    // The JSONL layer is the observability record of truth and `oxe trace`'s
    // only input: it gets its own fixed `info` floor so a restrictive
    // `OXE_LOG`/`RUST_LOG` (e.g. `warn`) cannot silently empty the logs.
    let json_layer = jsonl::JsonlLayer::new(writer).with_filter(LevelFilter::INFO);

    let stderr_layer = config
        .stderr_pretty
        .then(|| {
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_ansi(std::io::stderr().is_terminal())
                .with_writer(std::io::stderr)
        })
        .map(|layer| layer.with_filter(filter.clone()));

    // The OTLP layer is typed over `Registry`, so it is the first layer in
    // the stack. Its `EnvFilter` is a per-layer filter, like stderr's.
    #[cfg(feature = "otlp")]
    let (otlp_layer, otlp) = match config.otlp.then(otlp::build_layer).flatten() {
        Some((layer, handle)) => (Some(layer.with_filter(filter.clone())), Some(handle)),
        None => (None, None),
    };
    #[cfg(not(feature = "otlp"))]
    let otlp_layer: Option<tracing_subscriber::layer::Identity> = None;

    let subscriber = Registry::default()
        .with(otlp_layer)
        .with(json_layer)
        .with(stderr_layer);

    let guard = ObservabilityGuard {
        _writer_guard: writer_guard,
        #[cfg(feature = "otlp")]
        otlp,
    };
    Ok((tracing::Dispatch::new(subscriber), guard))
}

/// Install the observability pipeline as the global default subscriber.
/// Called once by `oxe serve` (and `oxe mcp`); must run before any spans are
/// created. The HTTP middleware that mints `RequestId`s is W0-09.
pub fn init(config: &ObservabilityConfig) -> Result<ObservabilityGuard, jsonl::LogInitError> {
    let (dispatch, guard) = build(config)?;
    dispatch.init();
    Ok(guard)
}

#[cfg(test)]
mod tests;
