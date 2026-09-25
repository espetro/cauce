//! `pages` conformance (W5-01): `put_page`/`get_page` round-trip and the
//! replace-on-conflict semantics the `pages_fts` triggers depend on.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::Utc;
use url::Url;

use crate::CacheKey;
use crate::store::{PageRow, Store};

/// `put_page` then `get_page` returns the row; a second `put_page` on the
/// same URL updates it (the upsert the `pages_fts_au` trigger rebuilds
/// the index from); an unknown URL reads `None`.
pub async fn pages_roundtrip(store: &impl Store) {
    let url = Url::parse("https://conf.example.com/conformance-page?a=1").unwrap();
    let row = PageRow {
        url: url.clone(),
        fetched_at: Utc::now(),
        title: "Conformance page".to_string(),
        markdown: "# Conformance\n\nalpha beta gamma".to_string(),
        byte_len: 1234,
        source_query_hash: None,
    };

    store.put_page(&row).await.expect("put_page");
    let got = store
        .get_page(&url)
        .await
        .expect("get_page")
        .expect("row must exist after put_page");
    assert_eq!(got.url, row.url);
    assert_eq!(got.title, row.title);
    assert_eq!(got.markdown, row.markdown);
    assert_eq!(got.byte_len, row.byte_len);
    assert_eq!(got.source_query_hash, None);

    // Replace: same URL key, new content, a `source_query_hash` this time.
    let key = CacheKey::from(&super::request("pages roundtrip replace"));
    let updated = PageRow {
        title: "Conformance page v2".to_string(),
        markdown: "# Conformance v2\n\ndelta epsilon".to_string(),
        byte_len: 999,
        source_query_hash: Some(key.clone()),
        ..row
    };
    store.put_page(&updated).await.expect("put_page replace");
    let got = store
        .get_page(&url)
        .await
        .expect("get_page after replace")
        .expect("row still there");
    assert_eq!(got.title, "Conformance page v2");
    assert_eq!(got.byte_len, 999);
    assert_eq!(got.source_query_hash.as_ref(), Some(&key));

    let missing = Url::parse("https://conf.example.com/never-indexed").unwrap();
    assert!(store.get_page(&missing).await.expect("get miss").is_none());
}
