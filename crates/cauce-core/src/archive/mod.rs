//! Fetch-and-index archive pipeline (W5-01, parent plan wave-5 subplan).
//!
//! [`Archiver::fetch_and_index`] is the single writer of the `pages` table:
//! [`Fetcher`] downloads the page under the archive's own host-keyed token
//! bucket, [`extract`] pulls the readable article out with `dom_smoothie`
//! and converts it to markdown with `htmd`, and the resulting
//! [`PageRow`] is upserted (the `pages_fts` triggers keep the lexical index
//! in step).
//!
//! Callers: `POST /api/pages`, the UI click beacon and the MCP
//! `fetch_and_index` tool — one code path, one politeness budget.

mod extract;
mod fetch;

use std::sync::Arc;

use chrono::Utc;
use url::Url;

use crate::CacheKey;
use crate::config::ArchiveConfig;
use crate::engine::EngineError;
use crate::normalize::normalize_url;
use crate::store::{PageRow, Store, StoreError};

pub use extract::extract;
pub use fetch::{FETCH_TIMEOUT, FetchedPage, Fetcher, MAX_FETCH_BYTES};

/// Stored-markdown cap (settled input: 200 KB). Longer extractions are cut
/// on a char boundary, with a `warn!` on the caller's span.
pub const MAX_MARKDOWN_BYTES: usize = 200 * 1024;

/// Every failure `fetch_and_index` can report.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// The URL does not parse or is not `http`/`https`.
    #[error("invalid url {0:?}: must be an absolute http(s) URL")]
    InvalidUrl(String),
    /// The network fetch failed (timeout, transport, DNS).
    #[error("fetch failed: {0}")]
    Fetch(EngineError),
    /// The upstream answered a non-2xx status.
    #[error("upstream returned status {0}")]
    Status(u16),
    /// Readability/conversion produced nothing usable.
    #[error("extraction failed: {0}")]
    Extract(String),
    /// The `pages` write failed.
    #[error("store write failed: {0}")]
    Store(#[from] StoreError),
}

/// The archive pipeline: fetcher + the `pages` writer.
///
/// Cloning is cheap (shared connection pool, bucket and store handle).
#[derive(Clone)]
pub struct Archiver {
    fetcher: Fetcher,
    store: Arc<dyn Store>,
}

impl std::fmt::Debug for Archiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Archiver").finish_non_exhaustive()
    }
}

impl Archiver {
    /// Build the pipeline from `[archive]` config: the host-keyed bucket is
    /// `requests_per_second`/`burst`, the rest is the shared politeness
    /// machinery. Never `None` on valid config — a zero bucket knob is a
    /// build error, matching `HttpClient`.
    pub fn new(store: Arc<dyn Store>, cfg: &ArchiveConfig) -> Result<Self, ArchiveError> {
        Ok(Self {
            fetcher: Fetcher::new(cfg.requests_per_second, cfg.burst)
                .map_err(ArchiveError::Fetch)?,
            store,
        })
    }

    /// Fetch `url`, extract the article to markdown and upsert the `pages`
    /// row. Returns the stored row.
    ///
    /// The row is keyed by the normalized final URL (after redirects), so a
    /// `GET /api/pages/{url}` on the same canonical form hits it; tracking
    /// params on the requested URL are stripped before fetch.
    pub async fn fetch_and_index(
        &self,
        url: &str,
        source_query_hash: Option<CacheKey>,
    ) -> Result<PageRow, ArchiveError> {
        let parsed = Url::parse(url)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| ArchiveError::InvalidUrl(url.to_string()))?;
        let requested = normalize_url(&parsed);

        let fetched = self
            .fetcher
            .get(requested.as_str())
            .await
            .map_err(ArchiveError::Fetch)?;
        if !(200..300).contains(&fetched.status) {
            return Err(ArchiveError::Status(fetched.status));
        }

        let html = String::from_utf8_lossy(&fetched.body);
        let page_url = fetched.url.as_str();
        let extracted = extract::extract(&html, page_url).map_err(ArchiveError::Extract)?;
        if extracted.markdown.trim().is_empty() {
            return Err(ArchiveError::Extract(
                "readability produced no readable content".to_string(),
            ));
        }

        let over_cap = extracted.markdown.len() > MAX_MARKDOWN_BYTES;
        let markdown = truncate_chars(extracted.markdown, MAX_MARKDOWN_BYTES);
        if over_cap {
            tracing::warn!(
                url = %fetched.url,
                cap = MAX_MARKDOWN_BYTES,
                "extracted markdown exceeds store cap; truncating"
            );
        }

        let row = PageRow {
            url: normalize_url(&fetched.url),
            fetched_at: Utc::now(),
            title: extracted.title,
            markdown,
            byte_len: fetched.body.len() as u64,
            source_query_hash,
        };
        self.store.put_page(&row).await?;
        Ok(row)
    }
}

/// Cut `s` to at most `cap` bytes on a char boundary (never mid-UTF-8).
fn truncate_chars(s: String, cap: usize) -> String {
    if s.len() <= cap {
        return s;
    }
    let mut end = cap;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_cuts_on_boundaries() {
        let s = "ab".repeat(300 * 1024);
        let cut = truncate_chars(s, MAX_MARKDOWN_BYTES);
        assert_eq!(cut.len(), MAX_MARKDOWN_BYTES);

        // Mid-UTF-8: 'é' is 2 bytes, cap lands inside it.
        let s = format!("{}é", "a".repeat(10));
        assert_eq!(truncate_chars(s, 11), "a".repeat(10));
        assert_eq!(truncate_chars("short".to_string(), 100), "short");
    }
}
