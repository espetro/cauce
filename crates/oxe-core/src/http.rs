//! HTTP egress for engine runtimes (W1-01, parent plan sections 4.1/4.3 and
//! the W1 "Settled inputs").
//!
//! [`HttpClient`] wraps `reqwest` (rustls, HTTP/2, connection pooling) with
//! the politeness policy every engine gets: a `governor` token bucket
//! (default 1 req/s, burst 3), one fixed `User-Agent` and
//! `Accept-Language` per engine (no rotation), a 2 MiB response cap, at
//! most 3 redirects, and a per-request timeout taken from the engine's
//! `budget`. Each upstream call runs inside an `engine_http` `tracing`
//! span recording `status`, `bytes` and `ms`.
//!
//! The [`Egress`] trait selects how traffic leaves the process: [`Direct`]
//! (default) or [`StaticProxy`] (`[engines.<id>.egress] proxy = "..."`).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue, USER_AGENT};
use tracing::{Instrument, debug, info_span};
use url::Url;

use crate::config::EgressConfig;
use crate::engine::{EngineError, EngineId};

/// Response body cap (settled input: 2 MB). Bodies larger than this are
/// aborted and surface as `EngineError::Parse` — a page that big is not a
/// search results page.
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum redirects followed per request (settled input: 3). Beyond the
/// cap the last `3xx` response is returned to the caller instead of
/// following on, so `detect` rules can still inspect it.
pub const MAX_REDIRECTS: usize = 3;

/// Token-bucket refill rate (settled input: 1 req/s).
pub const DEFAULT_REQUESTS_PER_SECOND: u32 = 1;

/// Token-bucket burst capacity (settled input: burst 3).
pub const DEFAULT_BURST: u32 = 3;

/// Default `Accept-Language` when an engine does not set its own.
pub const DEFAULT_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

/// Default `User-Agent` when an engine does not set its own. One fixed,
/// realistic UA per engine — no rotation (settled inputs).
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// How engine traffic leaves the process. Implementations adjust the
/// `reqwest::ClientBuilder` before the client is built.
pub trait Egress: Send + Sync + fmt::Debug {
    /// Apply this egress path to `builder` (e.g. install a proxy).
    fn configure(
        &self,
        builder: reqwest::ClientBuilder,
    ) -> Result<reqwest::ClientBuilder, EngineError>;
}

/// No proxy: requests go straight to the upstream.
#[derive(Debug, Default, Clone, Copy)]
pub struct Direct;

impl Egress for Direct {
    fn configure(
        &self,
        builder: reqwest::ClientBuilder,
    ) -> Result<reqwest::ClientBuilder, EngineError> {
        Ok(builder)
    }
}

/// Route every request through one fixed proxy (`http://`, `https://`,
/// `socks5://`, `socks5h://`). The URL is validated at construction.
#[derive(Debug, Clone)]
pub struct StaticProxy {
    url: String,
    proxy: reqwest::Proxy,
}

impl StaticProxy {
    /// Parse `url` into a `reqwest::Proxy` covering all schemes.
    pub fn new(url: impl Into<String>) -> Result<Self, EngineError> {
        let url = url.into();
        let proxy = reqwest::Proxy::all(&url)
            .map_err(|e| EngineError::Transport(format!("invalid proxy url {url:?}: {e}")))?;
        Ok(Self { url, proxy })
    }

    /// The proxy URL as configured.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Egress for StaticProxy {
    fn configure(
        &self,
        builder: reqwest::ClientBuilder,
    ) -> Result<reqwest::ClientBuilder, EngineError> {
        Ok(builder.proxy(self.proxy.clone()))
    }
}

/// Build the egress path from an `[engines.<id>.egress]` table. Absent
/// config or a missing `proxy` key means [`Direct`].
pub fn egress_from_config(cfg: Option<&EgressConfig>) -> Result<Box<dyn Egress>, EngineError> {
    match cfg.and_then(|c| c.proxy.as_deref()) {
        Some(url) => Ok(Box::new(StaticProxy::new(url)?)),
        None => Ok(Box::new(Direct)),
    }
}

/// Per-engine HTTP policy: the fixed identity headers and the token-bucket
/// politeness knobs (settled inputs: 1 req/s, burst 3, no UA rotation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPolicy {
    /// `User-Agent` sent on every request.
    pub user_agent: String,
    /// `Accept-Language` sent on every request.
    pub accept_language: String,
    /// Token-bucket refill rate in requests per second (>= 1).
    pub requests_per_second: u32,
    /// Token-bucket burst capacity (>= 1).
    pub burst: u32,
    /// Response body cap in bytes.
    pub max_response_bytes: usize,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_string(),
            accept_language: DEFAULT_ACCEPT_LANGUAGE.to_string(),
            requests_per_second: DEFAULT_REQUESTS_PER_SECOND,
            burst: DEFAULT_BURST,
            max_response_bytes: MAX_RESPONSE_BYTES,
        }
    }
}

impl HttpPolicy {
    /// Apply the bucket knobs from an `[engines.<id>.egress]` table (absent
    /// config keeps the defaults).
    pub fn with_egress_config(mut self, cfg: Option<&EgressConfig>) -> Self {
        if let Some(c) = cfg {
            self.requests_per_second = c.requests_per_second;
            self.burst = c.burst;
        }
        self
    }
}

/// One completed upstream response.
#[derive(Debug)]
pub struct HttpResponse {
    /// HTTP status code as received (not mapped; `detect` rules belong to
    /// the engine runtime, W1-02).
    pub status: u16,
    /// Final URL after redirects.
    pub url: Url,
    /// Response body, at most `policy.max_response_bytes`.
    pub body: Vec<u8>,
}

/// Per-engine HTTP client: `reqwest` (rustls, HTTP/2, pooled connections)
/// behind a `governor` token bucket. Cheap to clone: the connection pool
/// and the bucket are shared.
#[derive(Debug, Clone)]
pub struct HttpClient {
    engine: EngineId,
    client: reqwest::Client,
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    max_response_bytes: usize,
}

impl HttpClient {
    /// Build a client for `engine` through `egress` under `policy`.
    ///
    /// Fails on an unparsable header value, a `requests_per_second`/`burst`
    /// of zero, a bad proxy URL or a `reqwest` builder error.
    pub fn new(
        engine: EngineId,
        egress: &dyn Egress,
        policy: HttpPolicy,
    ) -> Result<Self, EngineError> {
        let rps = NonZeroU32::new(policy.requests_per_second).ok_or_else(|| {
            EngineError::Transport("egress.requests_per_second must be >= 1".to_string())
        })?;
        let burst = NonZeroU32::new(policy.burst)
            .ok_or_else(|| EngineError::Transport("egress.burst must be >= 1".to_string()))?;

        let mut headers = HeaderMap::new();
        for (name, value) in [
            (USER_AGENT, &policy.user_agent),
            (ACCEPT_LANGUAGE, &policy.accept_language),
        ] {
            let value = HeaderValue::from_str(value).map_err(|e| {
                EngineError::Transport(format!("invalid {name} header {value:?}: {e}"))
            })?;
            headers.insert(name, value);
        }

        let builder = reqwest::Client::builder()
            .default_headers(headers)
            // Cap at MAX_REDIRECTS hops; beyond it return the 3xx response
            // so engine `detect` rules still see the page it landed on.
            // `previous()` includes the URL that produced this redirect,
            // so `> MAX_REDIRECTS` allows exactly 3 follows.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > MAX_REDIRECTS {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .pool_max_idle_per_host(8)
            .tcp_keepalive(Duration::from_secs(60));
        let client = egress
            .configure(builder)?
            .build()
            .map_err(|e| EngineError::Transport(format!("http client build failed: {e}")))?;

        Ok(Self {
            engine,
            client,
            limiter: Arc::new(RateLimiter::direct(
                Quota::per_second(rps).allow_burst(burst),
            )),
            max_response_bytes: policy.max_response_bytes,
        })
    }

    /// The engine this client serves.
    pub fn engine(&self) -> &EngineId {
        &self.engine
    }

    /// `GET url` under `budget`: waits on the token bucket, then performs
    /// the request with `budget` as the whole-request timeout (connect,
    /// redirect chain and body read included).
    ///
    /// Errors map to [`EngineError`]: request timeouts to `Timeout`,
    /// transport failures to `Transport`, a body over
    /// `policy.max_response_bytes` to `Parse` (the abort path). HTTP error
    /// statuses are returned as `Ok` — status-to-error mapping is the
    /// engine runtime's `detect` contract.
    pub async fn get(&self, url: &str, budget: Duration) -> Result<HttpResponse, EngineError> {
        self.limiter.until_ready().await;

        let span = info_span!(
            "engine_http",
            engine = %self.engine,
            url = %url,
            status = tracing::field::Empty,
            bytes = tracing::field::Empty,
            ms = tracing::field::Empty,
            error = tracing::field::Empty,
        );
        let started = Instant::now();
        let result = self.send(url, budget).instrument(span.clone()).await;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        span.record("ms", ms);
        match &result {
            Ok(res) => {
                span.record("status", res.status);
                span.record("bytes", res.body.len());
            }
            Err(e) => {
                span.record("error", e.to_string());
                debug!(error = %e, "upstream call failed");
            }
        }
        result
    }

    async fn send(&self, url: &str, budget: Duration) -> Result<HttpResponse, EngineError> {
        let res = self
            .client
            .get(url)
            .timeout(budget)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        // Cheap early abort when the server announces an oversized body.
        if let Some(len) = res.content_length()
            && len > self.max_response_bytes as u64
        {
            return Err(size_cap_error(len as usize, self.max_response_bytes));
        }

        let status = res.status().as_u16();
        let final_url = res.url().clone();
        let mut body = Vec::new();
        let mut stream = res.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_reqwest_error)?;
            if body.len() + chunk.len() > self.max_response_bytes {
                return Err(size_cap_error(
                    body.len() + chunk.len(),
                    self.max_response_bytes,
                ));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            url: final_url,
            body,
        })
    }
}

fn size_cap_error(seen: usize, cap: usize) -> EngineError {
    EngineError::Parse(format!(
        "response body exceeds {cap} byte cap ({seen} bytes seen)"
    ))
}

fn map_reqwest_error(e: reqwest::Error) -> EngineError {
    if e.is_timeout() {
        EngineError::Timeout
    } else {
        EngineError::Transport(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_with(egress: &dyn Egress, policy: HttpPolicy) -> HttpClient {
        HttpClient::new(EngineId::new("test"), egress, policy).unwrap()
    }

    #[tokio::test]
    async fn token_bucket_delays_fourth_burst_call() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;
        // 4 req/s: the 4th call of a burst-3 bucket waits ~250 ms for a
        // token to refill.
        let client = client_with(
            &Direct,
            HttpPolicy {
                requests_per_second: 4,
                burst: 3,
                ..HttpPolicy::default()
            },
        );
        let url = format!("{}/r", server.uri());

        for _ in 0..3 {
            client.get(&url, Duration::from_secs(5)).await.unwrap();
        }
        let waited = Instant::now();
        client.get(&url, Duration::from_secs(5)).await.unwrap();
        let waited = waited.elapsed();
        assert!(
            waited >= Duration::from_millis(150),
            "4th burst call was not delayed by the bucket: {waited:?}"
        );
    }

    #[tokio::test]
    async fn static_proxy_routes_request_through_proxy() {
        // The "upstream" host does not exist; the request can only succeed
        // because the proxy answered it.
        let proxy = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
            .mount(&proxy)
            .await;
        let egress = StaticProxy::new(proxy.uri()).unwrap();
        let client = client_with(&egress, HttpPolicy::default());

        let res = client
            .get("http://upstream.invalid/search?q=x", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, b"via proxy");

        let seen = proxy.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1, "proxy saw {seen:?}");
        assert_eq!(seen[0].url.path(), "/search");
    }

    #[tokio::test]
    async fn response_size_cap_aborts_oversized_body() {
        let server = MockServer::start().await;
        let big = vec![b'x'; 3 * 1024 * 1024];
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(big))
            .mount(&server)
            .await;
        let client = client_with(&Direct, HttpPolicy::default());

        let err = client
            .get(&format!("{}/big", server.uri()), Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(
            matches!(err, EngineError::Parse(_)),
            "expected EngineError::Parse, got {err:?}"
        );
    }

    #[tokio::test]
    async fn redirect_cap_stops_after_three_hops() {
        let server = MockServer::start().await;
        // /r1 -> /r2 -> /r3 -> /final (3 hops) reaches the target...
        for (from, to) in [("/r1", "/r2"), ("/r2", "/r3"), ("/r3", "/final")] {
            Mock::given(method("GET"))
                .and(path(from))
                .respond_with(ResponseTemplate::new(302).insert_header("location", to))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_string("done"))
            .mount(&server)
            .await;
        let client = client_with(&Direct, HttpPolicy::default());
        let res = client
            .get(&format!("{}/r1", server.uri()), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body, b"done");

        // ...but a 4-hop chain returns the last redirect instead of
        // following it.
        Mock::given(method("GET"))
            .and(path("/r0"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/r1"))
            .mount(&server)
            .await;
        let res = client
            .get(&format!("{}/r0", server.uri()), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(res.status, 302);
        assert_eq!(res.url.path(), "/r3");
    }

    #[tokio::test]
    async fn request_budget_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        let client = client_with(&Direct, HttpPolicy::default());
        let err = client
            .get(
                &format!("{}/slow", server.uri()),
                Duration::from_millis(200),
            )
            .await
            .unwrap_err();
        assert_eq!(err, EngineError::Timeout);
    }

    #[test]
    fn invalid_proxy_url_fails_at_construction() {
        assert!(StaticProxy::new("not a url").is_err());
    }

    #[test]
    fn zero_rate_or_burst_rejected() {
        for policy in [
            HttpPolicy {
                requests_per_second: 0,
                ..HttpPolicy::default()
            },
            HttpPolicy {
                burst: 0,
                ..HttpPolicy::default()
            },
        ] {
            assert!(HttpClient::new(EngineId::new("t"), &Direct, policy).is_err());
        }
    }

    #[test]
    fn egress_from_config_defaults_to_direct() {
        assert!(egress_from_config(None).is_ok());
        let cfg = EgressConfig {
            proxy: Some("http://127.0.0.1:8888".to_string()),
            ..EgressConfig::default()
        };
        assert!(egress_from_config(Some(&cfg)).is_ok());
    }
}
