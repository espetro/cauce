//! Search log, click and merged-history conformance checks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use chrono::{DateTime, Utc};
use url::Url;

use super::{log_row, request};
use crate::cache::CacheKey;
use crate::request::ClientKind;
use crate::store::{ClickRow, HistoryFilter, HistoryItem, LogSource, Store};

/// `log_search`, `record_click` and the merged `list_history` feed with
/// `since`, `q` and `limit` filters.
pub async fn log_clicks_history(store: &impl Store) {
    let base = Utc::now();
    seed_searches_and_click(store, base).await;
    let history = verify_merged_feed(store).await;
    verify_history_filters(store, base).await;
    verify_delete_cascade(store, &history).await;
    seed_dup_rows(store, base).await;
    verify_dup_delete(store).await;
}

/// Seed two search rows, one pre-v2 row (`query_raw` NULL) and one click.
async fn seed_searches_and_click(store: &impl Store, base: DateTime<Utc>) {
    let alpha = "conformance history alpha";
    let beta = "conformance history beta";
    let click_url = "https://clicked.example.com/result";

    store
        .log_search(log_row(
            base - chrono::Duration::seconds(2),
            alpha,
            ClientKind::Api,
            LogSource::Network,
            640,
            10,
        ))
        .await
        .expect("log_search alpha");
    store
        .log_search(log_row(
            base - chrono::Duration::seconds(1),
            beta,
            ClientKind::Ui,
            LogSource::Cache,
            3,
            10,
        ))
        .await
        .expect("log_search beta");
    // A row as written before schema v2: `query_raw` stays NULL and must
    // decode back as `None`.
    let gamma = "conformance history gamma";
    let mut pre_v2 = log_row(base, gamma, ClientKind::Api, LogSource::Network, 1, 0);
    pre_v2.query_raw = None;
    store.log_search(pre_v2).await.expect("log_search gamma");
    store
        .record_click(ClickRow {
            id: None,
            ts: base,
            query_hash: Some(CacheKey::from(&request(alpha))),
            url: Url::parse(click_url).unwrap(),
            title: "Clicked result".to_string(),
            position: 0,
            client: ClientKind::Ui,
        })
        .await
        .expect("record_click");
}

/// The merged feed orders searches and clicks newest-first; `query_raw`
/// round-trips on schema-v2 rows and decodes `None` on older ones.
async fn verify_merged_feed(store: &impl Store) -> Vec<HistoryItem> {
    let alpha = "conformance history alpha";
    let beta = "conformance history beta";
    let click_url = "https://clicked.example.com/result";
    let gamma = "conformance history gamma";

    let history = store
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            cached: false,
            limit: 50,
        })
        .await
        .expect("list_history failed");

    let pos = |pred: &dyn Fn(&HistoryItem) -> bool| -> usize {
        history.iter().position(pred).expect("item in history")
    };
    let click_pos = pos(&|i| matches!(i, HistoryItem::Click(c) if c.url.as_str() == click_url));
    let beta_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == beta));
    let alpha_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == alpha));
    assert!(
        click_pos < beta_pos && beta_pos < alpha_pos,
        "history must merge searches and clicks newest-first"
    );

    // `query_raw` round-trips (schema v2); rows predating the column
    // decode it as None.
    let alpha_row = match &history[alpha_pos] {
        HistoryItem::Search(s) => s,
        _ => unreachable!(),
    };
    assert_eq!(alpha_row.query_raw.as_deref(), Some(alpha));
    let gamma_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == gamma));
    let gamma_row = match &history[gamma_pos] {
        HistoryItem::Search(s) => s,
        _ => unreachable!(),
    };
    assert_eq!(gamma_row.query_raw, None);

    history
}

/// `q` filters searches only (clicks pass through), `since` applies to both
/// item kinds and `limit` caps the merged feed.
async fn verify_history_filters(store: &impl Store, base: DateTime<Utc>) {
    let alpha = "conformance history alpha";
    let beta = "conformance history beta";
    let click_url = "https://clicked.example.com/result";

    // `q` filters searches only; clicks pass through.
    let filtered = store
        .list_history(&HistoryFilter {
            since: None,
            q: Some(alpha.to_string()),
            cached: false,
            limit: 50,
        })
        .await
        .expect("filtered history");
    assert!(
        filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == alpha))
    );
    assert!(
        !filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == beta))
    );
    assert!(
        filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Click(c) if c.url.as_str() == click_url)),
        "q filter must not remove clicks"
    );

    // `since` applies to both item kinds.
    let recent = store
        .list_history(&HistoryFilter {
            since: Some(base - chrono::Duration::milliseconds(1500)),
            q: None,
            cached: false,
            limit: 50,
        })
        .await
        .expect("since-filtered history");
    assert!(
        !recent
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == alpha))
    );
    assert!(
        recent
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == beta))
    );

    // `limit` caps the merged feed.
    let one = store
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            cached: false,
            limit: 1,
        })
        .await
        .expect("limited history");
    assert_eq!(one.len(), 1);
}

/// `delete_search_log` removes the row and cascades to the clicks that share
/// its `query_hash`; deleting a missing id is a `None`, not an error.
async fn verify_delete_cascade(store: &impl Store, history: &[HistoryItem]) {
    let alpha = "conformance history alpha";
    let beta = "conformance history beta";
    let click_url = "https://clicked.example.com/result";

    let alpha_id = history
        .iter()
        .find_map(|i| match i {
            HistoryItem::Search(s) if s.query == alpha => s.id,
            _ => None,
        })
        .expect("alpha row id");
    let outcome = store
        .delete_search_log(alpha_id)
        .await
        .expect("delete_search_log failed")
        .expect("existing row returns Some");
    assert_eq!(outcome.query, alpha);
    assert_eq!(outcome.clicks_removed, 1, "alpha's click cascades");

    let after = store
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            cached: false,
            limit: 50,
        })
        .await
        .expect("history after delete");
    assert!(
        !after
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == alpha)),
        "deleted search is gone"
    );
    assert!(
        !after
            .iter()
            .any(|i| matches!(i, HistoryItem::Click(c) if c.url.as_str() == click_url)),
        "the click went with its search"
    );
    assert!(
        after
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == beta)),
        "other rows survive"
    );
    assert!(
        store
            .delete_search_log(alpha_id)
            .await
            .expect("second delete")
            .is_none(),
        "deleting a missing id returns None"
    );
}

/// `suggest` completions (#150): case-insensitive prefix match on stored
/// queries, frecency order (use count first, most recent use breaking
/// ties), the `limit` cap and the empty-prefix short-circuit.
pub async fn suggest(store: &impl Store) {
    let base = Utc::now();
    // `beta` (two uses) outranks the more recent single-use rows; the
    // one-use rows order by recency. `echo`'s `query_raw` carries a
    // different casing — completions are the normalized `query` text.
    // `suggest-zed` shares no prefix and must never appear.
    for (q, offset_s) in [
        ("suggest-check beta", -10),
        ("suggest-check beta", -5),
        ("suggest-check alpha", 0),
        ("suggest-check gamma", -1),
        ("suggest-check echo", -2),
        ("suggest éclair unicode", -3),
        ("suggest-zed", 0),
    ] {
        let mut row = log_row(
            base + chrono::Duration::seconds(offset_s),
            q,
            ClientKind::Api,
            LogSource::Network,
            1,
            0,
        );
        if q == "suggest-check echo" {
            row.query_raw = Some("SUGGEST-CHECK Echo".to_string());
        }
        store.log_search(row).await.expect("log_search suggest");
    }

    let expected = vec![
        "suggest-check beta",
        "suggest-check alpha",
        "suggest-check gamma",
        "suggest-check echo",
    ];
    for prefix in ["suggest-check", "SUGGEST-CHECK"] {
        let got = store.suggest(prefix, 10).await.expect("suggest failed");
        assert_eq!(got, expected, "suggest({prefix:?})");
    }

    // `prefix` is normalised the same way stored queries were written:
    // unicode case and whitespace runs fold before the prefix match.
    let got = store
        .suggest("  SUGGEST  ÉCLAIR  ", 10)
        .await
        .expect("suggest normalised");
    assert_eq!(got, vec!["suggest éclair unicode"], "suggest unicode");
    let got = store
        .suggest("suggest-check  beta", 10)
        .await
        .expect("suggest whitespace");
    assert_eq!(got, vec!["suggest-check beta"], "suggest whitespace");

    let capped = store
        .suggest("suggest-check", 2)
        .await
        .expect("suggest limit");
    assert_eq!(capped, expected[..2].to_vec(), "suggest honouring limit");

    for prefix in ["", "zz-nothing"] {
        let got = store.suggest(prefix, 10).await.expect("suggest empty");
        assert!(got.is_empty(), "suggest({prefix:?}) must be empty: {got:?}");
    }
}

/// Seed two search rows sharing one query plus a click on that query.
async fn seed_dup_rows(store: &impl Store, base: DateTime<Utc>) {
    // Two rows sharing a query: deleting the older one keeps the clicks
    // (the surviving newest row still displays them); deleting the last
    // row takes them.
    let dup = "conformance history dup";
    store
        .log_search(log_row(
            base,
            dup,
            ClientKind::Api,
            LogSource::Network,
            640,
            10,
        ))
        .await
        .expect("log_search dup 1");
    store
        .log_search(log_row(
            base + chrono::Duration::seconds(1),
            dup,
            ClientKind::Ui,
            LogSource::Cache,
            3,
            10,
        ))
        .await
        .expect("log_search dup 2");
    store
        .record_click(ClickRow {
            id: None,
            ts: base + chrono::Duration::seconds(2),
            query_hash: Some(CacheKey::from(&request(dup))),
            url: Url::parse("https://clicked.example.com/dup").unwrap(),
            title: "Dup click".to_string(),
            position: 0,
            client: ClientKind::Ui,
        })
        .await
        .expect("record_click dup");
}

/// Deleting the older of two same-hash rows keeps the clicks; the last row
/// takes them. `search_hashes` reports which hashes still have a row.
async fn verify_dup_delete(store: &impl Store) {
    let beta = "conformance history beta";
    let dup = "conformance history dup";

    let dup_rows = store
        .list_history(&HistoryFilter {
            since: None,
            q: Some(dup.to_string()),
            cached: false,
            limit: 50,
        })
        .await
        .expect("dup history")
        .into_iter()
        .filter_map(|i| match i {
            HistoryItem::Search(s) if s.query == dup => s.id,
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dup_rows.len(), 2, "two dup rows logged");
    // Feed is newest-first: [0] is the newer row, [1] the older.
    let (newer_id, older_id) = (dup_rows[0], dup_rows[1]);

    let outcome = store
        .delete_search_log(older_id)
        .await
        .expect("delete older dup")
        .expect("existing row");
    assert_eq!(
        outcome.clicks_removed, 0,
        "clicks survive while a row shares the hash"
    );
    let outcome = store
        .delete_search_log(newer_id)
        .await
        .expect("delete last dup")
        .expect("existing row");
    assert_eq!(
        outcome.clicks_removed, 1,
        "the last row for the hash takes the clicks"
    );

    // `search_hashes` reports which hashes still have a `search_log` row.
    let present = store
        .search_hashes(&[
            CacheKey::from(&request(dup)),
            CacheKey::from(&request(beta)),
        ])
        .await
        .expect("search_hashes");
    assert!(
        present.iter().any(|k| *k == CacheKey::from(&request(beta))),
        "beta's row is present"
    );
    assert!(
        !present.iter().any(|k| *k == CacheKey::from(&request(dup))),
        "dup's rows are gone"
    );
}
