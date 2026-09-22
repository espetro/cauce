//! Span contract for `SearchPipeline`: every stage carries `request_id`
//! so `oxe trace` can rebuild the fan-out. Asserted here with a capturing
//! fmt subscriber; the JSONL rendering contract is oxe-server's own test
//! suite.
//!
//! Lives in its own test binary on purpose: `tracing` callsite interest is
//! cached process-wide, so a sibling test evaluating the same callsites
//! under no subscriber would poison them for this one.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::io;
use std::sync::{Arc, Mutex};

use oxe_core::SearchPipeline;
use support::{StubStore, replay_at, req};
use uuid::Uuid;

#[test]
fn spans_carry_request_id() {
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
    let request_id = Uuid::now_v7();

    tracing::dispatcher::with_default(&dispatch, || {
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let engine = replay_at(dir.path(), |_| {});
            let store = Arc::new(StubStore::default());
            let pipe = SearchPipeline::new(store, vec![Arc::new(engine)]);
            pipe.search_with_id(&req("span test"), request_id)
                .await
                .unwrap();
        });
    });

    let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    for needle in [
        "pipeline.search",
        "cache_lookup",
        "engine",
        "merge",
        "persist",
        "request_id",
        &request_id.to_string(),
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}
