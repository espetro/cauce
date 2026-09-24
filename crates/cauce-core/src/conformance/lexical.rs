//! Tier-2 lexical lookup (`get_lexical`) conformance checks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use chrono::Utc;

use super::{request, response};
use crate::cache::CacheKey;
use crate::store::Store;

/// `get_lexical` finds entries by query text, result titles and result
/// snippets (FTS5 over `cache_fts` for the sqlite impl).
pub async fn lexical_search(store: &impl Store) {
    let key = CacheKey::from(&request("conformance lexical"));
    let resp = response(
        "conformance lexical",
        &[
            (
                "TanStack Router documentation",
                "https://tanstack.com/router",
                "type safe routing",
            ),
            (
                "Unrelated result",
                "https://example.com/other",
                "nothing here",
            ),
        ],
    );
    store
        .put(&key, &resp, Duration::from_secs(3600))
        .await
        .expect("put failed");

    // Acceptance: a title mention is found by a lowercase term.
    let hits = store
        .get_lexical("tanstack", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical(\"tanstack\") must find the row whose title mentions TanStack"
    );

    // Snippet-only term also matches.
    let hits = store
        .get_lexical("routing", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical must index result snippets"
    );

    // Query-column term also matches.
    let hits = store
        .get_lexical("conformance", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical must index the stored query"
    );

    // A term present nowhere yields no rows.
    let hits = store
        .get_lexical("qzxwvunmatchable", 10)
        .await
        .expect("get_lexical failed");
    assert!(hits.is_empty(), "gibberish term must match nothing");

    // Punctuation-only input is an empty result, not an FTS syntax error.
    let hits = store
        .get_lexical("*", 10)
        .await
        .expect("get_lexical('*') failed");
    assert!(hits.is_empty(), "operator-only input must not error");

    // Pinned semantic: get_lexical DOES return expired rows (callers check
    // CachedSearch::expires_at to serve or mark stale). W6 Postgres must match.
    let expired_key = CacheKey::from(&request("conformance lexical expired"));
    store
        .put(
            &expired_key,
            &response(
                "conformance lexical expired",
                &[("stale zephyr hit", "https://stale.example.com/", "x")],
            ),
            Duration::ZERO,
        )
        .await
        .expect("put expired");
    let hits = store
        .get_lexical("zephyr", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter()
            .any(|c| c.key == expired_key && c.expires_at <= Utc::now()),
        "get_lexical must return expired rows (stale serving is the caller's call)"
    );

    // A zero-result response is still indexed (its query column) and the FTS
    // row carries the cache_entries rowid, not a phantom.
    let empty_key = CacheKey::from(&request("conformance lexical empty"));
    store
        .put(
            &empty_key,
            &response("conformance lexical empty", &[]),
            Duration::from_secs(3600),
        )
        .await
        .expect("put empty");
    let hits = store
        .get_lexical("empty", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == empty_key),
        "empty-results rows must be indexed and found via their query"
    );

    // Lexical results must survive the source row being deleted.
    assert!(store.delete_cache(&key).await.expect("delete_cache failed"));
    let hits = store
        .get_lexical("tanstack", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().all(|c| c.key != key),
        "deleted rows must leave the lexical index"
    );
}
