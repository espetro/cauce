//! `CacheKey` (sha256 of the canonical request preimage), `CachedSearch`
//! (the `cache_entries` row), and query normalisation.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::EngineId;
use crate::request::{SafeSearch, SearchRequest, TimeRange};
use crate::response::SearchResponse;

/// Query normalisation used by `CacheKey` and the `replay` cassette key:
/// unicode-aware lowercase, all whitespace runs collapsed to a single ASCII
/// space, edges trimmed.
pub fn normalize_query(q: &str) -> String {
    q.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// English function words removed before the tier-2 token comparison
/// (W1-10: "normalisation and stopword removal"). Deliberately small and
/// generic; content words (`docs`, `query`, `api`, ...) are never stopwords.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "can", "could", "did", "do",
    "does", "for", "from", "had", "has", "have", "how", "i", "if", "in", "into", "is", "it", "its",
    "me", "my", "no", "not", "of", "on", "or", "our", "she", "so", "that", "the", "their", "them",
    "then", "there", "these", "they", "this", "to", "too", "up", "us", "was", "we", "what", "when",
    "where", "which", "who", "will", "with", "you", "your",
];

/// The token set a tier-2 candidate is scored against: `normalize_query`,
/// split on non-alphanumeric boundaries (mirroring the FTS5 unicode61
/// tokenizer that built `cache_fts`), stopwords dropped.
pub(crate) fn lexical_tokens(q: &str) -> BTreeSet<String> {
    normalize_query(q)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .filter(|t| !STOPWORDS.contains(t))
        .map(str::to_string)
        .collect()
}

/// Jaccard similarity |A∩B| / |A∪B| over two token sets. Two empty sets
/// score 0.0: a query of nothing but stopwords never matches.
pub(crate) fn token_jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

/// sha256 hex digest identifying a cacheable request.
///
/// Canonical preimage (section 4.2): sha256 over a length-prefixed binary
/// tuple, fields in fixed order:
///
/// ```text
///   normalize_query(q)                    u32le len + utf8
///   page                                  u8
///   lang                                  0x00 | 0x01 + u32le len + utf8
///   time_range                            0x00 | 0x01 + u8 (day=1 week=2 month=3 year=4)
///   safesearch                            u8 (off=0 moderate=1 strict=2)
///   engines                               0x00 when None
///                                         | 0x01 + u32le count + sorted, deduped,
///                                           length-prefixed engine ids
/// ```
///
/// Engine ids are sorted before hashing, so the key is stable under
/// permutation of `engines`. `engines = None` is *not* the same key as
/// `engines = Some(<default set>)`: the option tag byte differs, which is the
/// pinned-engines rule — an explicit pin never shares a cache entry with the
/// default fan-out.
///
/// # Examples
///
/// ```
/// use oxe_core::{CacheKey, ClientKind, EngineId, SafeSearch, SearchRequest};
///
/// let req = SearchRequest {
///     q: "tanstack router".into(),
///     page: 1,
///     lang: None,
///     time_range: None,
///     safesearch: SafeSearch::Off,
///     engines: None,
///     client: ClientKind::Api,
/// };
///
/// // engines=None twice is equal.
/// assert_eq!(CacheKey::from(&req), CacheKey::from(&req));
///
/// // engines=None vs engines=Some(<default set>) is NOT equal: a pinned
/// // request gets its own cache entries.
/// let mut pinned = req.clone();
/// pinned.engines = Some(vec![EngineId::from("bing"), EngineId::from("brave")]);
/// assert_ne!(CacheKey::from(&req), CacheKey::from(&pinned));
///
/// // Engine order does not matter.
/// let mut permuted = req.clone();
/// permuted.engines = Some(vec![EngineId::from("brave"), EngineId::from("bing")]);
/// assert_eq!(CacheKey::from(&pinned), CacheKey::from(&permuted));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CacheKey(String);

impl CacheKey {
    /// Hex digest (64 lowercase chars). This is the `key` column of
    /// `cache_entries` and the `{key}` path segment of `/api/cache/{key}`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CacheKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self(s.to_lowercase()))
        } else {
            Err(format!(
                "invalid cache key: expected 64 hex chars, got {s:?}"
            ))
        }
    }
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

fn push_opt_str(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        None => buf.push(0),
        Some(s) => {
            buf.push(1);
            push_str(buf, s);
        }
    }
}

impl From<&SearchRequest> for CacheKey {
    fn from(req: &SearchRequest) -> Self {
        let mut buf = Vec::with_capacity(64);
        push_str(&mut buf, &normalize_query(&req.q));
        buf.push(req.page);
        push_opt_str(&mut buf, req.lang.as_deref());
        buf.push(match req.time_range {
            None => 0,
            Some(TimeRange::Day) => 1,
            Some(TimeRange::Week) => 2,
            Some(TimeRange::Month) => 3,
            Some(TimeRange::Year) => 4,
        });
        buf.push(match req.safesearch {
            SafeSearch::Off => 0,
            SafeSearch::Moderate => 1,
            SafeSearch::Strict => 2,
        });
        match &req.engines {
            None => buf.push(0),
            Some(ids) => {
                buf.push(1);
                let mut sorted: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
                sorted.sort_unstable();
                sorted.dedup();
                buf.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
                for id in sorted {
                    push_str(&mut buf, id);
                }
            }
        }
        let digest = Sha256::digest(&buf);
        Self(format!("{digest:x}"))
    }
}

/// A `cache_entries` row (parent plan section 5).
///
/// `get_exact` only returns rows where `expires_at` is still in the future;
/// `get_cache` (admin route) returns the row regardless.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearch {
    pub key: CacheKey,
    /// The query as stored (`response.query` normalised form is in `params`).
    pub query: String,
    /// Request parameters that produced this entry (`params_json` column).
    pub params: serde_json::Value,
    /// Stored payload (`payload_json` column, decoded).
    pub response: SearchResponse,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Number of times this row served a hit.
    pub hits: u64,
    /// Engines that produced the stored response (`engines_json` column).
    pub engines: Vec<EngineId>,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::request::ClientKind;

    fn req(engines: Option<Vec<EngineId>>) -> SearchRequest {
        SearchRequest {
            q: "test query".to_string(),
            page: 1,
            lang: None,
            time_range: None,
            safesearch: SafeSearch::Moderate,
            engines,
            client: ClientKind::Api,
        }
    }

    #[test]
    fn query_normalisation_collapses_whitespace_and_case() {
        assert_eq!(normalize_query("  Foo\t BAR\n baz "), "foo bar baz");
        assert_eq!(
            CacheKey::from(&SearchRequest {
                q: "  Foo\t BAR ".to_string(),
                ..req(None)
            }),
            CacheKey::from(&SearchRequest {
                q: "foo bar".to_string(),
                ..req(None)
            })
        );
    }

    #[test]
    fn every_key_field_changes_the_key() {
        let base = req(None);
        let key = CacheKey::from(&base);
        for changed in [
            SearchRequest {
                page: 2,
                ..base.clone()
            },
            SearchRequest {
                lang: Some("en".to_string()),
                ..base.clone()
            },
            SearchRequest {
                time_range: Some(TimeRange::Week),
                ..base.clone()
            },
            SearchRequest {
                safesearch: SafeSearch::Strict,
                ..base.clone()
            },
        ] {
            assert_ne!(key, CacheKey::from(&changed));
        }
    }

    #[test]
    fn lexical_tokens_normalise_split_and_drop_stopwords() {
        // Case, whitespace and punctuation collapse; stopwords leave.
        let toks = lexical_tokens("  The TanStack  ROUTER-docs! ");
        assert_eq!(
            toks.iter().map(String::as_str).collect::<Vec<_>>(),
            ["docs", "router", "tanstack"]
        );
        // Content words that look like engine vocabulary are kept.
        assert!(lexical_tokens("docs query api").contains("docs"));
        assert!(lexical_tokens("docs query api").contains("query"));
        // A pure-stopword query yields an empty set.
        assert!(lexical_tokens("the a an of").is_empty());
    }

    #[test]
    fn token_jaccard_matches_the_w1_10_acceptance_numbers() {
        // Issue #30: `docs tanstack router` vs stored `tanstack router docs`
        // scores 1.0; `tanstack query docs` scores 0.5.
        let stored = lexical_tokens("tanstack router docs");
        assert_eq!(
            token_jaccard(&lexical_tokens("docs tanstack router"), &stored),
            1.0
        );
        assert_eq!(
            token_jaccard(&lexical_tokens("tanstack query docs"), &stored),
            0.5
        );
        // Identical text after stopword removal also scores 1.0.
        assert_eq!(
            token_jaccard(&lexical_tokens("the tanstack router docs"), &stored),
            1.0
        );
        // Empty sets never match, even two of them.
        assert_eq!(
            token_jaccard(&lexical_tokens("the a"), &lexical_tokens("of to")),
            0.0
        );
    }

    #[test]
    fn key_roundtrips_hex() {
        let key = CacheKey::from(&req(None));
        assert_eq!(key.as_str().len(), 64);
        assert_eq!(CacheKey::from_str(key.as_str()).unwrap(), key);
        assert!(CacheKey::from_str("not-a-key").is_err());
    }

    proptest! {
        #[test]
        fn key_stable_under_engine_order_permutation(
            engines in prop::collection::vec("[a-z]{2,12}", 1..6),
        ) {
            let pinned = engines.iter().map(EngineId::from).collect::<Vec<_>>();
            let mut shuffled = pinned.clone();
            // Reverse is a permutation; for idempotent good measure also dedup.
            shuffled.reverse();
            let a = CacheKey::from(&req(Some(pinned)));
            let b = CacheKey::from(&req(Some(shuffled)));
            prop_assert_eq!(a, b);
        }

        #[test]
        fn key_none_never_equals_some(
            engines in prop::collection::vec("[a-z]{2,12}", 0..6),
        ) {
            let unpinned = CacheKey::from(&req(None));
            let pinned = CacheKey::from(&req(Some(
                engines.iter().map(EngineId::from).collect(),
            )));
            prop_assert_ne!(unpinned, pinned);
        }
    }
}
