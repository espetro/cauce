//! Cache-area conformance checks: exact round-trip, expiry/eviction and the
//! admin surface (`list_cache`, `get_cache`, `delete_cache`, `clear_cache`).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use chrono::Utc;

use super::{request, response};
use crate::cache::CacheKey;
use crate::store::Store;

/// `put` then `get_exact` round-trips a `SearchResponse`; a second `put`
/// replaces the row.
pub async fn cache_exact_roundtrip(store: &impl Store) {
    let key = CacheKey::from(&request("conformance exact roundtrip"));
    let resp = response(
        "conformance exact roundtrip",
        &[
            ("Alpha result", "https://example.com/alpha", "alpha snippet"),
            ("Beta result", "https://example.com/beta", "beta snippet"),
        ],
    );

    store
        .put(&key, &resp, Duration::from_secs(3600))
        .await
        .expect("put failed");

    let got = store
        .get_exact(&key)
        .await
        .expect("get_exact failed")
        .expect("fresh row must be visible to get_exact");

    assert_eq!(got.key, key);
    assert_eq!(got.query, resp.query);
    assert_eq!(got.response, resp, "payload must round-trip exactly");
    assert!(got.expires_at > Utc::now(), "fresh row must be unexpired");
    assert!(got.hits >= 1, "a served hit must be counted");
    assert!(
        got.engines.iter().any(|e| e.as_str() == "conf-engine"),
        "engines_json must carry the producing engines"
    );

    // Replace: same key, new payload.
    let resp2 = response(
        "conformance exact roundtrip",
        &[("Gamma result", "https://example.com/gamma", "gamma snippet")],
    );
    store
        .put(&key, &resp2, Duration::from_secs(3600))
        .await
        .expect("second put failed");
    let got = store
        .get_exact(&key)
        .await
        .expect("get_exact failed")
        .expect("replaced row must still be visible");
    assert_eq!(got.response, resp2, "put must replace the stored payload");

    // An absurd TTL must clamp to a representable timestamp, not corrupt the
    // row (saturating to i64::MAX overflows DateTime::from_timestamp_millis).
    let huge_key = CacheKey::from(&request("conformance exact huge ttl"));
    store
        .put(&huge_key, &resp, Duration::MAX)
        .await
        .expect("put with Duration::MAX failed");
    let got = store
        .get_exact(&huge_key)
        .await
        .expect("get_exact failed")
        .expect("clamped row must decode, not read back Corrupt");
    assert!(got.expires_at > Utc::now());
}

/// Rows past `expires_at` are invisible to `get_exact`, visible to the admin
/// `get_cache`, and removed by `evict_expired` — but only once they are
/// `grace` past expiry (the W3-02 stale-serve window).
pub async fn cache_expiry_and_eviction(store: &impl Store) {
    let key = CacheKey::from(&request("conformance expired row"));
    let resp = response(
        "conformance expired row",
        &[("Old result", "https://example.com/old", "old snippet")],
    );

    // A zero TTL expires immediately.
    store
        .put(&key, &resp, Duration::ZERO)
        .await
        .expect("put failed");

    assert!(
        store
            .get_exact(&key)
            .await
            .expect("get_exact failed")
            .is_none(),
        "expired rows must be invisible to get_exact"
    );
    assert!(
        store
            .get_cache(&key)
            .await
            .expect("get_cache failed")
            .is_some(),
        "admin get_cache must still see expired rows"
    );

    // The W3-02 stale-serve grace: a row expired inside the window is not
    // garbage — eviction must spare it.
    store
        .evict_expired(Duration::from_secs(3600))
        .await
        .expect("evict_expired failed");
    assert!(
        store
            .get_cache(&key)
            .await
            .expect("get_cache failed")
            .is_some(),
        "in-grace expired row survives eviction"
    );

    let evicted = store
        .evict_expired(Duration::ZERO)
        .await
        .expect("evict_expired failed");
    assert!(evicted >= 1, "evict_expired must report the removed row");

    assert!(
        store
            .get_cache(&key)
            .await
            .expect("get_cache failed")
            .is_none(),
        "evicted row must be gone for good"
    );
}

/// Cache admin surface: `list_cache` (newest first, includes expired),
/// `get_cache`, `delete_cache`, `clear_cache`.
pub async fn cache_admin(store: &impl Store) {
    let k1 = CacheKey::from(&request("conformance admin a"));
    let k2 = CacheKey::from(&request("conformance admin b"));
    let r1 = response(
        "conformance admin a",
        &[("A", "https://a.example.com/", "a")],
    );
    let r2 = response(
        "conformance admin b",
        &[("B", "https://b.example.com/", "b")],
    );

    store
        .put(&k1, &r1, Duration::from_secs(3600))
        .await
        .expect("put k1");
    // k2 is already expired: list_cache must still include it.
    store.put(&k2, &r2, Duration::ZERO).await.expect("put k2");

    let all = store.list_cache(50, 0).await.expect("list_cache failed");
    let p1 = all.iter().position(|c| c.key == k1).expect("k1 listed");
    let p2 = all
        .iter()
        .position(|c| c.key == k2)
        .expect("expired k2 listed");
    assert!(
        p2 < p1,
        "list_cache must be newest-first (k2 inserted after k1)"
    );

    // Pagination: page two continues where page one stopped.
    let page0 = store.list_cache(1, 0).await.expect("list_cache page0");
    let page1 = store.list_cache(1, 1).await.expect("list_cache page1");
    assert_eq!(page0.len(), 1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page0[0].key, all[0].key);
    assert_eq!(page1[0].key, all[1].key);

    assert!(store.delete_cache(&k1).await.expect("delete k1"));
    assert!(
        !store.delete_cache(&k1).await.expect("delete missing"),
        "deleting a missing key must return false"
    );

    let removed = store.clear_cache().await.expect("clear_cache failed");
    assert!(removed >= 1, "clear_cache must report removed rows");
    assert!(
        store
            .list_cache(50, 0)
            .await
            .expect("list after clear")
            .is_empty(),
        "clear_cache empties the table"
    );
}
