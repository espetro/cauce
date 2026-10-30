//! Threads: the UX-01 `answer_thread` surface — upsert, owner-scoped
//! reads (a foreign owner 404s, never leaks), newest-first listing and
//! delete.

use chrono::Utc;
use uuid::Uuid;

use crate::ai::AnswerRole;
use crate::store::{AnswerThreadRow, Store};

/// A two-turn thread row for `owner`. `offset_ms` spaces `updated_at`
/// so listing order is deterministic.
fn thread_row(owner: &str, title: &str, offset_ms: i64) -> AnswerThreadRow {
    let ts = Utc::now() + chrono::Duration::milliseconds(offset_ms);
    AnswerThreadRow {
        id: Uuid::now_v7(),
        owner: owner.to_string(),
        title: title.to_string(),
        created_at: ts,
        updated_at: ts,
        turns: vec![
            crate::ai::AnswerTurn {
                role: AnswerRole::User,
                content: "what is cauce?".to_string(),
            },
            crate::ai::AnswerTurn {
                role: AnswerRole::Assistant,
                content: "a metasearch engine.".to_string(),
            },
        ],
    }
}

pub async fn threads_roundtrip(store: &impl Store) {
    let mine = thread_row("conf-owner", "what is cauce?", 0);
    let other = thread_row("conf-owner-other", "another user thread", 1);

    // Upsert is idempotent on id: a re-upsert (the next completed turn)
    // replaces the row, it does not duplicate.
    store
        .upsert_thread(mine.clone())
        .await
        .expect("upsert mine");
    store
        .upsert_thread(other.clone())
        .await
        .expect("upsert other");

    let got = store
        .get_thread(&mine.id.to_string(), "conf-owner")
        .await
        .expect("get_thread")
        .expect("own thread readable");
    assert_eq!(got.id, mine.id);
    assert_eq!(got.owner, "conf-owner");
    assert_eq!(got.turns.len(), 2);
    assert_eq!(got.turns[0].role, AnswerRole::User);

    // Owner scoping: another owner gets `None` (the 404 rule), and
    // deleting a foreign id is a false, not an error.
    assert!(
        store
            .get_thread(&mine.id.to_string(), "conf-owner-other")
            .await
            .expect("scoped get")
            .is_none(),
        "a thread must not be readable by another owner"
    );
    assert!(
        !store
            .delete_thread(&mine.id.to_string(), "conf-owner-other")
            .await
            .expect("scoped delete"),
        "a thread must not be deletable by another owner"
    );

    // Re-upsert with the same id replaces the row.
    let mut grown = mine.clone();
    grown.turns.push(crate::ai::AnswerTurn {
        role: AnswerRole::User,
        content: "and its license?".to_string(),
    });
    grown.turns.push(crate::ai::AnswerTurn {
        role: AnswerRole::Assistant,
        content: "MPL-2.0.".to_string(),
    });
    grown.updated_at = Utc::now();
    store.upsert_thread(grown).await.expect("re-upsert");
    let got = store
        .get_thread(&mine.id.to_string(), "conf-owner")
        .await
        .expect("get after re-upsert")
        .expect("thread still present");
    assert_eq!(got.turns.len(), 4, "re-upsert must replace the transcript");
}

pub async fn threads_listing_and_delete(store: &impl Store) {
    let rows: Vec<_> = (0..3)
        .map(|i| thread_row("conf-list-owner", &format!("thread {i}"), i * 1000))
        .collect();
    for row in &rows {
        store.upsert_thread(row.clone()).await.expect("upsert");
    }
    let listed = store
        .list_threads("conf-list-owner", 10)
        .await
        .expect("list_threads");
    let ids: Vec<_> = listed.iter().map(|r| r.id).collect();
    for row in &rows {
        assert!(ids.contains(&row.id), "listing must contain every thread");
    }
    // Newest-first: `updated_at` descending. The last-inserted row
    // carries the largest offset, so it leads; verify the inserted
    // prefix is ordered rather than asserting whole-list equality
    // (pre-existing rows from other areas must not break the run).
    let inserted: Vec<_> = rows.iter().map(|r| r.id).collect();
    let pos: Vec<usize> = inserted
        .iter()
        .map(|id| listed.iter().position(|r| r.id == *id).expect("listed"))
        .collect();
    let mut sorted = pos.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(pos, sorted, "rows must list newest-first");

    // A foreign owner lists none of them.
    let foreign = store
        .list_threads("conf-list-nobody", 10)
        .await
        .expect("foreign list");
    assert!(
        foreign.iter().all(|r| !inserted.contains(&r.id)),
        "another owner must list none of these threads"
    );

    // Delete removes it; the second delete reports false.
    assert!(
        store
            .delete_thread(&rows[0].id.to_string(), "conf-list-owner")
            .await
            .expect("delete"),
        "own delete must report true"
    );
    assert!(
        !store
            .delete_thread(&rows[0].id.to_string(), "conf-list-owner")
            .await
            .expect("second delete"),
        "a second delete must report false"
    );
    assert!(
        store
            .get_thread(&rows[0].id.to_string(), "conf-list-owner")
            .await
            .expect("get after delete")
            .is_none(),
        "deleted thread must be gone"
    );
}
