//! W3-02 rule c acceptance: a stale serve while every pinned engine is
//! unhealthy (breaker open or skipped) emits a warn event and bumps
//! `cauce_stale_served_total{reason="engines_unhealthy"}`.
//!
//! Lives in its own test binary on purpose: `tracing` callsite interest
//! is cached process-wide, so a sibling test evaluating the same
//! callsites under no subscriber would poison them for this one.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cauce_core::{
    CacheKey, EngineError, EngineId, EngineReport, EngineStatus, SearchMeta, SearchPipeline,
    SearchResponse, SearchResult, Source,
};
use chrono::Utc;
use support::{StubStore, replay_at, req};
use url::Url;
use uuid::Uuid;

#[test]
fn stale_serve_with_unhealthy_engines_warns() {
    #[derive(Clone)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buf {
        type Writer = Buf;
        fn make_writer(&'a self) -> Buf {
            self.clone()
        }
    }

    let buf = Buf(Arc::new(Mutex::new(Vec::new())));
    let dispatch = tracing::Dispatch::new(
        tracing_subscriber::fmt()
            .with_writer(buf.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    tracing::dispatcher::with_default(&dispatch, || {
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let engine = replay_at(dir.path(), |_| {});
            let store = Arc::new(StubStore::default());
            let pipe = SearchPipeline::new(store.clone(), vec![Arc::new(engine)]);

            // Seed a row expired inside the stale-serve grace.
            let stale_req = req("unhealthy engines");
            let key = CacheKey::from(&stale_req);
            let seeded = SearchResponse {
                query: "unhealthy engines".to_string(),
                results: vec![SearchResult {
                    url: Url::parse("https://stale.example/").unwrap(),
                    title: "stale".to_string(),
                    snippet: "expired row".to_string(),
                    engine: EngineId::new("replay"),
                    published: None,
                    score: 1.0,
                }],
                meta: SearchMeta {
                    source: Source::Network,
                    engines_used: vec![EngineReport {
                        engine: EngineId::new("replay"),
                        status: EngineStatus::Ok,
                        latency_ms: 1,
                        result_count: 1,
                    }],
                    engines_skipped: Vec::new(),
                    deadline_hit: false,
                    hedged: false,
                    hedge_at_ms: None,
                    elapsed_ms: 1,
                    request_id: Uuid::now_v7(),
                },
            };
            store.entries.lock().unwrap().insert(
                key.as_str().to_string(),
                (
                    seeded,
                    Utc::now() - chrono::Duration::hours(2),
                    Duration::from_secs(1),
                ),
            );

            // Simulated all-breaker outage: `Blocked` opens the only
            // pinned engine's breaker.
            pipe.health().record_err(
                &EngineId::from("replay"),
                Duration::ZERO,
                &EngineError::Blocked,
                Uuid::now_v7(),
            );

            let resp = pipe.search(&stale_req).await.expect("stale serve");
            assert!(
                matches!(resp.meta.source, Source::Cache { stale: true, .. }),
                "expected a stale serve, got {:?}",
                resp.meta.source
            );
        });
    });

    let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains("every pinned engine is unhealthy"),
        "expected the engines_unhealthy warn event in:\n{text}"
    );

    let prom = cauce_core::metrics::render_prometheus();
    assert!(
        prom.contains("cauce_stale_served_total{reason=\"engines_unhealthy\"}"),
        "expected the engines_unhealthy counter in:\n{prom}"
    );
}
