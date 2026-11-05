#![cfg(feature = "conformance")]
//! Conformance tests run against `cauce_core::MemoryStore`.

use cauce_core::MemoryStore;
use cauce_core::conformance::*;

#[tokio::test]
async fn memory_store_conformance() {
    let store = MemoryStore::new();
    run_all(&store).await;
}
