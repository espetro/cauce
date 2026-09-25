//! `pages` conformance (W5-01): `put_page`/`get_page` round-trip and the
//! replace-on-conflict semantics the `pages_fts` triggers depend on, plus
//! (W5-02) `search_pages`/`list_pages`/`delete_page`.
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

/// W5-02: `search_pages` finds a stored page by a body phrase with a
/// marked snippet and a score, `list_pages` serves newest-first scoreless
/// excerpts, and `delete_page` evicts the row from both `pages` and
/// `pages_fts` (searching after the delete must not see it).
pub async fn pages_search_and_delete(store: &impl Store) {
    let url = Url::parse("https://conf.example.com/conformance-search?a=1").unwrap();
    let row = PageRow {
        url: url.clone(),
        fetched_at: Utc::now(),
        title: "Conformance search page".to_string(),
        markdown: "# Conformance\n\nquixotic zephyr diligence".to_string(),
        byte_len: 640,
        source_query_hash: None,
    };
    store.put_page(&row).await.expect("put_page");

    let hits = store
        .search_pages("zephyr diligence", 10)
        .await
        .expect("search_pages");
    let hit = hits
        .iter()
        .find(|h| h.url == url)
        .expect("search_pages must find the stored page by body phrase");
    assert!(
        hit.snippet_parts().iter().any(|(_, marked)| *marked),
        "search_pages snippets must carry match marks: {:?}",
        hit.snippet
    );
    let marks = hit
        .snippet
        .chars()
        .filter(|c| *c == crate::PAGE_MARK_OPEN || *c == crate::PAGE_MARK_CLOSE)
        .count();
    assert!(
        marks >= 2,
        "a marked term opens and closes: {:?}",
        hit.snippet
    );
    assert_eq!(
        hit.plain_snippet().chars().count(),
        hit.snippet.chars().count() - marks,
        "plain_snippet strips exactly the mark delimiters"
    );
    assert!(hit.score.is_some(), "search hits carry a bm25 score");

    let listed = store.list_pages(10, 0).await.expect("list_pages");
    let row = listed
        .iter()
        .find(|h| h.url == url)
        .expect("list_pages must include the stored page");
    assert!(row.score.is_none(), "listing rows carry no score");
    assert!(
        !row.snippet.is_empty(),
        "listing rows carry a markdown excerpt"
    );

    assert!(store.delete_page(&url).await.expect("delete_page"));
    assert!(
        store
            .get_page(&url)
            .await
            .expect("get after delete")
            .is_none()
    );
    assert!(
        !store
            .search_pages("zephyr diligence", 10)
            .await
            .expect("search after delete")
            .iter()
            .any(|h| h.url == url),
        "delete_page must evict the row from the FTS index"
    );
    assert!(
        !store.delete_page(&url).await.expect("repeat delete"),
        "deleting a missing row reports false"
    );
}
