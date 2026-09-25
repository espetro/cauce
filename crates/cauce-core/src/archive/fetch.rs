//! Archive page fetcher: the same politeness machinery as
//! [`crate::http::HttpClient`] — fixed UA/`Accept-Language`, a `governor`
//! token bucket, capped redirects, a whole-request timeout, a streamed body
//! cap — except the bucket is keyed by host (W5-01 settled input) so
//! archive traffic to one host never starves another, and an oversized body
//! is *truncated* (logged, still indexed) rather than aborted like engine
//! egress.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue, USER_AGENT};
use tracing::{Instrument, debug, info_span, warn};
use url::Url;

use crate::engine::EngineError;
use crate::http::{DEFAULT_ACCEPT_LANGUAGE, DEFAULT_USER_AGENT, MAX_REDIRECTS, map_reqwest_error};

/// Fetched-body cap (settled input: 1 MB). A page bigger than this is cut at
/// the cap and indexed truncated, with a `warn!` marking `truncated = true`
/// on the `archive_fetch` span.
pub const MAX_FETCH_BYTES: usize = 1024 * 1024;

/// Whole-request timeout for archive fetches (connect, redirects and body
/// included). Deliberately shorter than a search deadline: click beacons
/// fire-and-forget and `POST /api/pages`/MCP callers block on this.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// One fetched page: status, final URL and the (possibly truncated) body.
#[derive(Debug)]
pub struct FetchedPage {
    /// HTTP status as received; the caller maps non-2xx to its error.
    pub status: u16,
    /// Final URL after redirects (the URL that actually served `body`).
    pub url: Url,
    /// Response body, at most [`MAX_FETCH_BYTES`].
    pub body: Vec<u8>,
    /// `true` when the upstream body ran past the cap and `body` was cut.
    pub truncated: bool,
}

type HostBucket = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// HTTP fetcher for the archive pipeline. Cheap to clone: connection pool
/// and the host-keyed bucket are shared.
#[derive(Debug, Clone)]
pub struct Fetcher {
    client: reqwest::Client,
    limiter: Arc<HostBucket>,
}

impl Fetcher {
    /// Build the fetcher with `crate::http` politeness defaults and a
    /// host-keyed token bucket of `requests_per_second`/`burst`.
    pub fn new(requests_per_second: u32, burst: u32) -> Result<Self, EngineError> {
        let rps = NonZeroU32::new(requests_per_second).ok_or_else(|| {
            EngineError::Transport("archive.requests_per_second must be >= 1".to_string())
        })?;
        let burst = NonZeroU32::new(burst)
            .ok_or_else(|| EngineError::Transport("archive.burst must be >= 1".to_string()))?;

        let mut headers = HeaderMap::new();
        for (name, value) in [
            (USER_AGENT, DEFAULT_USER_AGENT),
            (ACCEPT_LANGUAGE, DEFAULT_ACCEPT_LANGUAGE),
        ] {
            let hv = HeaderValue::from_str(value).map_err(|e| {
                EngineError::Transport(format!("invalid default header {value:?}: {e}"))
            })?;
            headers.insert(name, hv);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > MAX_REDIRECTS {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .pool_max_idle_per_host(4)
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| EngineError::Transport(format!("http client build failed: {e}")))?;

        Ok(Self {
            client,
            limiter: Arc::new(RateLimiter::keyed(
                Quota::per_second(rps).allow_burst(burst),
            )),
        })
    }

    /// `GET url`: waits on the host's token bucket, then performs the
    /// request with [`FETCH_TIMEOUT`] as the whole-request timeout. The
    /// body streams and is cut at [`MAX_FETCH_BYTES`]; `truncated` reports
    /// the cut.
    ///
    /// Errors map like [`crate::http::HttpClient`]: request timeouts to
    /// `Timeout`, transport failures to `Transport`, a URL that does not
    /// parse or has no host to `Parse`. Non-2xx statuses return `Ok` —
    /// status-to-error mapping is the caller's contract.
    pub async fn get(&self, url: &str) -> Result<FetchedPage, EngineError> {
        let key = Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        self.limiter.until_key_ready(&key).await;

        let span = info_span!(
            "archive_fetch",
            url = %url,
            host = %key,
            status = tracing::field::Empty,
            bytes = tracing::field::Empty,
            truncated = tracing::field::Empty,
            ms = tracing::field::Empty,
            error = tracing::field::Empty,
        );
        let started = Instant::now();
        let result = self.send(url).instrument(span.clone()).await;
        span.record("ms", started.elapsed().as_millis() as u64);
        match &result {
            Ok(res) => {
                span.record("status", res.status);
                span.record("bytes", res.body.len());
                span.record("truncated", res.truncated);
            }
            Err(e) => {
                span.record("error", e.to_string());
                debug!(error = %e, "archive fetch failed");
            }
        }
        result
    }

    async fn send(&self, url: &str) -> Result<FetchedPage, EngineError> {
        let res = self
            .client
            .get(url)
            .timeout(FETCH_TIMEOUT)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = res.status().as_u16();
        let final_url = res.url().clone();

        // An announced oversized body still streams to the cap — the early
        // Content-Length check would only skip the read, and the settled
        // contract is to index what was fetched (capped + logged), not to
        // refuse the page.
        let mut body = Vec::new();
        let mut stream = res.bytes_stream();
        let mut truncated = false;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_reqwest_error)?;
            if body.len() + chunk.len() > MAX_FETCH_BYTES {
                let room = MAX_FETCH_BYTES - body.len();
                body.extend_from_slice(&chunk[..room]);
                truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        if truncated {
            warn!(
                url = %url,
                cap = MAX_FETCH_BYTES,
                "page body exceeds fetch cap; indexing truncated content"
            );
        }
        Ok(FetchedPage {
            status,
            url: final_url,
            body,
            truncated,
        })
    }
}
