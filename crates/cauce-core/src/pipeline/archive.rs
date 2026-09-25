//! `search_archive` (W5-03): hybrid RRF over `pages_fts` and `cache_fts`.
//!
//! Two ranked lists — `Store::search_pages` hits and `Store::search_cache_fts`
//! result hits — are fused by [`RrfMerge`] with dedupe on the normalized URL,
//! so a URL both indexed *and* cached is one [`ArchiveHit`] boosted by both
//! lists (settled judgment: fuse by URL, boost the shared doc). `k` and the
//! per-host collapse follow the pipeline's [`MergePolicy`]; both lists carry
//! equal weight — local stores have no engine reliability history to weigh.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::cache::normalize_query;
use crate::engine::EngineId;
use crate::response::SearchResult;
use crate::store::StoreError;

use super::SearchPipeline;
use super::merge::RrfMerge;

/// Which ranked list produced an [`ArchiveHit`] — the `source` field of
/// the settled `{url, title, snippet, source, score}` wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveSource {
    /// `pages_fts` — a fetched-and-indexed page.
    Page,
    /// `cache_fts` — a result stored inside a cached `SearchResponse`.
    CachedResult,
}

impl ArchiveSource {
    /// The `engine` marker carried through the merge's `SearchResult`
    /// shape: `EngineId` accepts `[A-Za-z0-9._-]+`, so the source survives
    /// the RRF round-trip. A fused hit reports the list whose single
    /// contribution ranked best (the page list wins ties — it is added
    /// first).
    fn marker(self) -> EngineId {
        EngineId::from(match self {
            Self::Page => "page",
            Self::CachedResult => "cached_result",
        })
    }

    fn from_marker(id: &EngineId) -> Self {
        match id.as_str() {
            "cached_result" => Self::CachedResult,
            _ => Self::Page,
        }
    }
}

/// One `search_archive` result (the settled wire item).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveHit {
    /// The hit URL — the normalized form when it appeared in both lists.
    pub url: Url,
    /// Page readability title or the stored result title.
    pub title: String,
    /// Plain-text excerpt: page hits go through
    /// [`PageHit::plain_snippet`](crate::PageHit::plain_snippet) (match
    /// marks stripped); cached results carry the stored snippet verbatim.
    pub snippet: String,
    /// Which list produced (or best-ranked) the hit.
    pub source: ArchiveSource,
    /// Fused RRF score (`merge.rrf_k`, summed over the lists the URL
    /// appeared in).
    pub score: f32,
}

impl SearchPipeline {
    /// Hybrid archive search: the `pages_fts` and `cache_fts` ranked lists
    /// fused by [`RrfMerge`], capped at `limit` hits. Both store reads run
    /// concurrently; a failure in either fails the call — a silently
    /// dropped list would rank wrong rather than merely return less.
    pub async fn search_archive(&self, q: &str, limit: u32) -> Result<Vec<ArchiveHit>, StoreError> {
        let q = normalize_query(q);
        let (pages, cached) = tokio::try_join!(
            self.store.search_pages(&q, limit),
            self.store.search_cache_fts(&q, limit),
        )?;
        let page_results: Vec<SearchResult> = pages
            .into_iter()
            .map(|hit| SearchResult {
                snippet: hit.plain_snippet(),
                url: hit.url,
                title: hit.title,
                engine: ArchiveSource::Page.marker(),
                published: Some(hit.fetched_at),
                score: 0.0,
            })
            .collect();
        let cached_results: Vec<SearchResult> = cached
            .into_iter()
            .map(|hit| SearchResult {
                url: hit.url,
                title: hit.title,
                snippet: hit.snippet,
                engine: ArchiveSource::CachedResult.marker(),
                published: None,
                score: 0.0,
            })
            .collect();
        let mut merge = RrfMerge::new(
            self.merge.rrf_k,
            vec![1.0, 1.0],
            self.merge.collapse_same_host_after,
        );
        merge.add(0, &page_results);
        merge.add(1, &cached_results);
        Ok(merge
            .finish()
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|r| ArchiveHit {
                url: r.url,
                title: r.title,
                snippet: r.snippet,
                source: ArchiveSource::from_marker(&r.engine),
                score: r.score,
            })
            .collect())
    }
}
