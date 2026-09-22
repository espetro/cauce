//! Process metrics provider (W1-09): the OTel `SdkMeterProvider` feeding
//! `GET /metrics` (Prometheus text) and, when `OTEL_EXPORTER_OTLP_ENDPOINT`
//! is set, a periodic OTLP push reader.
//!
//! [`MetricsHandle`] owns the meter provider, the Prometheus `Registry` the
//! handler scrapes and the `oxe_cache_entries` gauge cell. The provider is
//! also installed as the process-global meter provider, so `oxe_core::Metrics`
//! handles created with `Metrics::default()` (the pipeline, engine runtimes)
//! bind to it on their first record.
//!
//! `oxe_cache_entries` is an observable gauge over a refreshed cell rather
//! than a direct `Store` read: OTel callbacks are synchronous and cannot
//! await the async store. The `/metrics` handler refreshes it before each
//! scrape and a 30 s background task keeps it current for push exports.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use oxe_core::{Metrics, Store};
use prometheus::Registry;

/// How often the `oxe_cache_entries` cell is refreshed outside scrapes.
const CACHE_GAUGE_INTERVAL: Duration = Duration::from_secs(30);

/// Prometheus exposition content type for `GET /metrics`.
pub const METRICS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Owns the metrics pipeline for the process: provider, pull registry,
/// cache gauge cell and its refresh task.
///
/// Construction is deliberately infallible at the signature level: a
/// misconfigured exporter degrades to an empty (still parseable) registry
/// rather than failing `oxe serve` startup, mirroring how OTLP trace export
/// already treats exporter errors as non-fatal.
#[derive(Clone)]
pub struct MetricsHandle {
    inner: Arc<Inner>,
}

struct Inner {
    registry: Registry,
    provider: SdkMeterProvider,
    /// The bound instrument set; pipelines may take it via
    /// `SearchPipeline::with_metrics`, though the lazy global binding makes
    /// that optional.
    metrics: Metrics,
    store: Arc<dyn Store>,
    /// Backing cell of `oxe_cache_entries`.
    cache_entries: Arc<AtomicU64>,
    /// Periodic refresh of `cache_entries` for OTLP push exports; `None`
    /// when built without a tokio runtime (unit tests built sync-side).
    refresh_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Dedicated runtime keeping the tonic channel worker alive when the
    /// OTLP metrics exporter was built without an ambient one.
    _otlp_runtime: Option<tokio::runtime::Runtime>,
}

/// The OTLP metrics reader (periodic push), or `None` when
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is unset or construction fails (never
/// fatal — same policy as `observability::otlp`).
///
/// The returned runtime keeps tonic's channel worker alive when no ambient
/// runtime existed at build time.
#[cfg(feature = "otlp")]
fn otlp_reader() -> Option<(
    opentelemetry_sdk::metrics::PeriodicReader<opentelemetry_otlp::MetricExporter>,
    Option<tokio::runtime::Runtime>,
)> {
    std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT")?;

    // tonic's `connect_lazy` spawns its buffer worker with `tokio::spawn`
    // at build time; enter a runtime for the build. When the caller has no
    // runtime (CLI paths), keep a dedicated one alive in the handle.
    let mut owned_runtime = None;
    let _entered;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        _entered = handle.enter();
    } else {
        match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
        {
            Ok(rt) => {
                _entered = rt.enter();
                owned_runtime = Some(rt);
            }
            Err(e) => {
                eprintln!("oxe: metrics: cannot create OTLP exporter runtime: {e}");
                return None;
            }
        }
    }

    match opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .build()
    {
        Ok(exporter) => Some((
            opentelemetry_sdk::metrics::PeriodicReader::builder(exporter).build(),
            owned_runtime,
        )),
        Err(e) => {
            eprintln!("oxe: metrics: OTLP exporter build failed: {e}");
            None
        }
    }
}

impl MetricsHandle {
    /// Build the provider and install it globally. `store` backs the
    /// `oxe_cache_entries` gauge refresh.
    pub fn new(store: Arc<dyn Store>) -> Self {
        let registry = Registry::new();
        let mut builder = SdkMeterProvider::builder()
            .with_resource(Resource::builder().with_service_name("oxe").build());
        match opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            // The instruments already carry the settled `_total`/`_ms`
            // names; the exporter must not append its own conventions.
            .without_counter_suffixes()
            .without_units()
            .build()
        {
            Ok(reader) => builder = builder.with_reader(reader),
            Err(e) => eprintln!("oxe: metrics: prometheus exporter build failed: {e}"),
        }

        #[cfg(feature = "otlp")]
        let otlp_runtime = match otlp_reader() {
            Some((reader, runtime)) => {
                builder = builder.with_reader(reader);
                runtime
            }
            None => None,
        };
        #[cfg(not(feature = "otlp"))]
        let otlp_runtime: Option<tokio::runtime::Runtime> = None;

        let provider = builder.build();
        opentelemetry::global::set_meter_provider(provider.clone());
        let meter = provider.meter("oxe");
        let metrics = Metrics::new(&meter);

        // `oxe_cache_entries`: observable gauge over a refreshed cell —
        // OTel callbacks are synchronous and cannot await the async store.
        let cache_entries = Arc::new(AtomicU64::new(0));
        let cell = Arc::clone(&cache_entries);
        meter
            .u64_observable_gauge("oxe_cache_entries")
            .with_description("Live (unexpired) cache_entries rows")
            .with_callback(move |observer| {
                observer.observe(cell.load(Ordering::Relaxed), &[]);
            })
            .build();

        let inner = Arc::new(Inner {
            registry,
            provider,
            metrics,
            store,
            cache_entries,
            refresh_task: Mutex::new(None),
            _otlp_runtime: otlp_runtime,
        });

        // Keep the cell fresh for OTLP push exports when a runtime is
        // available (the /metrics scrape refreshes it on demand too).
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let inner_ref = Arc::clone(&inner);
            let task = runtime.spawn(async move {
                let mut tick = tokio::time::interval(CACHE_GAUGE_INTERVAL);
                tick.tick().await; // first tick is immediate; skip it
                loop {
                    tick.tick().await;
                    if let Ok(stats) = inner_ref.store.stats(0).await {
                        inner_ref
                            .cache_entries
                            .store(stats.cache_entries, Ordering::Relaxed);
                    }
                }
            });
            *inner.refresh_task.lock().unwrap() = Some(task);
        }

        Self { inner }
    }

    /// The bound [`Metrics`] instrument set (for `with_metrics` wiring).
    pub fn metrics(&self) -> Metrics {
        self.inner.metrics.clone()
    }

    /// Refresh the `oxe_cache_entries` cell from the store. Called by the
    /// `/metrics` handler before each scrape and by the refresh task.
    pub async fn refresh_cache(&self) {
        // days=0 reads the unbounded window; only the cache counts are used.
        if let Ok(stats) = self.inner.store.stats(0).await {
            self.inner
                .cache_entries
                .store(stats.cache_entries, Ordering::Relaxed);
        }
    }

    /// Prometheus text exposition of everything collected so far.
    pub fn render(&self) -> Result<String, prometheus::Error> {
        use prometheus::Encoder;
        let families = self.inner.registry.gather();
        let mut buf = Vec::new();
        prometheus::TextEncoder::new().encode(&families, &mut buf)?;
        String::from_utf8(buf).map_err(|e| prometheus::Error::Msg(e.to_string()))
    }

    /// Flush readers and stop the refresh task. Called once at serve
    /// shutdown; `Drop` covers the test/degraded paths.
    pub fn shutdown(&self) {
        if let Some(task) = self.inner.refresh_task.lock().unwrap().take() {
            task.abort();
        }
        let _ = self.inner.provider.shutdown();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(task) = self.refresh_task.get_mut().unwrap().take() {
            task.abort();
        }
        let _ = self.provider.shutdown();
    }
}
