//! Escaping-helper proptests, run against a real in-memory SQLite: a
//! `like_pattern` output must be a literal substring match under
//! `LIKE ... ESCAPE '\'`, and a `Some` `fts_query` output must parse and run
//! as an FTS5 `MATCH` expression.
//!
//! Inputs exclude embedded NUL: SQLite truncates bound text at the first
//! NUL inside `LIKE`/`MATCH` string arguments, so no escaping can carry one
//! through either expression.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use proptest::prelude::*;
use rusqlite::{Connection, params};

use super::*;

proptest! {
    /// `haystack LIKE like_pattern(needle) ESCAPE '\'` holds iff `haystack`
    /// literally contains `needle`: metacharacters in the input never act as
    /// wildcards. `case_sensitive_like` pins byte-exact semantics.
    #[test]
    fn like_pattern_is_a_literal_substring_match(
        needle in ".*".prop_filter("no NUL", |s| !s.contains('\0')),
        haystack in ".*".prop_filter("no NUL", |s| !s.contains('\0')),
    ) {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "case_sensitive_like", true)
            .unwrap();
        let matched: bool = conn
            .query_row(
                "SELECT ?1 LIKE ?2 ESCAPE '\\'",
                params![haystack, like_pattern(&needle)],
                |r| r.get(0),
            )
            .unwrap();
        prop_assert_eq!(matched, haystack.contains(&needle));
    }

    /// `fts_query` never panics and yields `None` or an expression a real
    /// FTS5 table accepts as `MATCH`.
    #[test]
    fn fts_query_is_valid_fts5(q in ".*".prop_filter("no NUL", |s| !s.contains('\0'))) {
        if let Some(expr) = fts_query(&q) {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute("CREATE VIRTUAL TABLE t USING fts5(x)", [])
                .unwrap();
            let ran = conn
                .prepare("SELECT rowid FROM t WHERE t MATCH ?1")
                .and_then(|mut s| {
                    s.query_map(params![expr], |r| r.get::<_, i64>(0))
                        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
                });
            prop_assert!(ran.is_ok(), "FTS5 rejected {expr:?}: {:?}", ran.err());
        }
    }
}
