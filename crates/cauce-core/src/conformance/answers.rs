//! `answers`-area conformance checks (W4-02): round-trip, TTL honouring
//! and upsert-replace semantics of `get_answer`/`put_answer`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use chrono::Utc;
use url::Url;

use crate::engine::EngineId;
use crate::store::{AnswerKey, AnswerPayload, AnswerRow, AnswerSource, Store};

fn answer_row(query: &str, model: &str, confidence: u8) -> AnswerRow {
    AnswerRow {
        query: query.to_string(),
        model: model.to_string(),
        payload: AnswerPayload {
            answer: "the answer".to_string(),
            confidence,
            related_questions: vec!["conformance related?".to_string()],
        },
        sources: vec![AnswerSource {
            url: Url::parse("https://example.com/conf-answer").expect("fixture url parses"),
            title: "Alpha result".to_string(),
            snippet: "alpha snippet".to_string(),
            engine: EngineId::from("conf-engine"),
        }],
    }
}

/// `put_answer` then `get_answer` round-trips the row; a second
/// `put_answer` replaces it; an expired row is invisible to `get_answer`.
pub async fn answers_roundtrip(store: &impl Store) {
    let query = "conformance answers roundtrip";
    let key = AnswerKey::new(query, "conf-model");
    let row = answer_row(query, "conf-model", 8);

    store
        .put_answer(&key, &row, Duration::from_secs(3600))
        .await
        .expect("put_answer failed");

    let got = store
        .get_answer(&key)
        .await
        .expect("get_answer failed")
        .expect("fresh row must be visible to get_answer");

    assert_eq!(got.key, key);
    assert_eq!(got.query, query);
    assert_eq!(got.model, "conf-model");
    assert_eq!(got.payload.answer, "the answer");
    assert_eq!(got.payload.confidence, 8);
    assert_eq!(got.payload.related_questions, ["conformance related?"]);
    assert_eq!(got.sources.len(), 1);
    assert_eq!(
        got.sources[0].url.as_str(),
        "https://example.com/conf-answer"
    );
    assert!(got.expires_at > Utc::now(), "fresh row must be unexpired");

    // Model is part of the key: the same query under another model misses.
    let other = AnswerKey::new(query, "other-model");
    assert!(
        store
            .get_answer(&other)
            .await
            .expect("get_answer failed")
            .is_none(),
        "a different model must not see the row"
    );

    // Replace: same key, new payload.
    let mut updated = row.clone();
    updated.payload.answer = "the revised answer".to_string();
    store
        .put_answer(&key, &updated, Duration::from_secs(3600))
        .await
        .expect("second put_answer failed");
    let got = store
        .get_answer(&key)
        .await
        .expect("get_answer failed")
        .expect("replaced row must be visible");
    assert_eq!(got.payload.answer, "the revised answer");

    // Expired rows are invisible (TTL honoured at read time).
    let stale_key = AnswerKey::new("conformance answers expired", "conf-model");
    let stale = answer_row("conformance answers expired", "conf-model", 8);
    store
        .put_answer(&stale_key, &stale, Duration::ZERO)
        .await
        .expect("put_answer failed");
    assert!(
        store
            .get_answer(&stale_key)
            .await
            .expect("get_answer failed")
            .is_none(),
        "an expired row must be invisible to get_answer"
    );
}
