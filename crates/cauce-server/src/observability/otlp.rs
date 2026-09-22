//! OTLP trace export (`otlp` cargo feature, non-default since W1-12 so
//! tonic/prost stay out of the default build).
//!
//! Inert unless `OTEL_EXPORTER_OTLP_ENDPOINT` is set at init time. Exports
//! spans in batches over OTLP/gRPC (tonic). tonic spawns its channel worker
//! with `tokio::spawn` when the exporter is built, so the exporter must be
//! built inside a tokio runtime context and that runtime must stay alive;
//! when no runtime is current we create a dedicated one-thread runtime that
//! the guard keeps alive.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::Registry;

/// The `tracing` layer bridging spans to OTLP. Its subscriber type is
/// `Registry`, so it must be layered directly on `Registry::default()`.
pub type OtelLayer =
    tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::SdkTracer>;

/// Owns the tracer provider (and the tokio runtime when we had to create
/// one). `shutdown` flushes the batch queue.
pub struct OtlpHandle {
    provider: SdkTracerProvider,
    /// Kept alive so the tonic channel worker keeps running. `None` when an
    /// ambient runtime already existed at init (e.g. under `tokio::main`).
    _runtime: Option<tokio::runtime::Runtime>,
}

impl OtlpHandle {
    /// Flush and shut down the exporter. `SdkTracerProvider::shutdown` is
    /// blocking; on a `current_thread` runtime that can deadlock, so run it
    /// on a helper thread there.
    pub fn shutdown(self) {
        let provider = self.provider;
        let in_current_thread = tokio::runtime::Handle::try_current()
            .map(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread)
            .unwrap_or(false);
        if in_current_thread {
            std::thread::spawn(move || {
                let _ = provider.shutdown();
            });
        } else {
            let _ = provider.shutdown();
        }
    }
}

/// Build the OTLP pipeline, or `None` when `OTEL_EXPORTER_OTLP_ENDPOINT`
/// is unset or exporter construction fails (never fatal).
pub fn build_layer() -> Option<(OtelLayer, OtlpHandle)> {
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
                eprintln!("cauce: otlp: cannot create exporter runtime: {e}");
                return None;
            }
        }
    }

    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
    {
        Ok(exporter) => exporter,
        Err(e) => {
            eprintln!("cauce: otlp: exporter build failed: {e}");
            return None;
        }
    };

    let provider = SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name("cauce").build())
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer("cauce");
    opentelemetry::global::set_tracer_provider(provider.clone());

    Some((
        tracing_opentelemetry::layer().with_tracer(tracer),
        OtlpHandle {
            provider,
            _runtime: owned_runtime,
        },
    ))
}
