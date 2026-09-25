//! Archive page fetcher: the same politeness machinery as
//! [`crate::http::HttpClient`] — fixed UA/`Accept-Language`, a `governor`
//! token bucket, capped redirects, a whole-request timeout, a streamed body
//! cap — except the bucket is keyed by host (W5-01 settled input) so
//! archive traffic to one host never starves another, and an oversized body
//! is *truncated* (logged, still indexed) rather than aborted like engine
//! egress.
//!
//! SSRF egress guard (#189): `http`/`https` only, and the redirect chain
//! is unrolled by hand so each hop is validated before it is dialed.
//! Literal-IP hosts never reach a DNS resolver, so [`check_url`] rejects
//! private/reserved literals on every hop; DNS names resolve through
//! [`GuardedResolver`], which refuses a host when *any* resolved address
//! is private/reserved (loopback, RFC 1918, link-local, CGNAT,
//! multicast, ...) and returns exactly the checked addresses to the dial
//! — no DNS TOCTOU. A hop into private space fails the fetch with
//! [`FetchError::Blocked`] instead of being followed silently.
//! `[archive] allow_private` opts out for indexing local services.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{ACCEPT_LANGUAGE, HeaderMap, HeaderValue, USER_AGENT};
use tracing::{Instrument, debug, info_span, warn};
use url::{Host, Url};

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

/// Every failure [`Fetcher::get`] can report. `Blocked` is a caller
/// fault (the API maps it to 4xx); `Engine` carries the shared
/// timeout/transport vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The egress guard refused the request: the URL scheme is not
    /// `http`/`https`, or a resolved address — on the first hop or any
    /// redirect — is in a private/reserved range.
    #[error("blocked by egress guard: {0}")]
    Blocked(String),
    /// The fetch itself failed (timeout, transport, DNS).
    #[error("{0}")]
    Engine(#[from] EngineError),
}

/// The marker error [`GuardedResolver`] returns on a private/reserved
/// resolution. [`Fetcher::send`] recovers it from the `reqwest` error
/// source chain so the guard rejection stays a `Blocked`, not a generic
/// transport failure.
#[derive(Debug)]
struct EgressBlocked {
    host: String,
    addr: IpAddr,
}

impl fmt::Display for EgressBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "host {:?} resolves to private/reserved address {}",
            self.host, self.addr
        )
    }
}

impl std::error::Error for EgressBlocked {}

/// `true` when `ip` is a public unicast address the archive fetcher may
/// dial. IPv4-mapped IPv6 (`::ffff:a.b.c.d`) applies the v4 rules so a
/// mapped loopback cannot slip through as a v6 literal.
fn is_allowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(&v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(&v4),
            None => is_public_v6(&v6),
        },
    }
}

fn is_public_v4(v4: &Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    !(v4.is_loopback()      // 127/8
        || v4.is_private()    // 10/8, 172.16/12, 192.168/16
        || v4.is_link_local() // 169.254/16
        || v4.is_multicast()  // 224/4
        || v4.is_broadcast()  // 255.255.255.255
        || v4.is_documentation() // 192.0.2/24, 198.51.100/24, 203.0.113/24
        || v4.is_unspecified()   // 0.0.0.0
        // Ranges std still gates nightly: 0/8 "this network", CGNAT
        // 100.64/10 (`is_shared`), 192.0.0/24 IETF protocol assignments,
        // 198.18/15 benchmarking (`is_benchmarking`), 240/4 reserved
        // (`is_reserved`).
        || a == 0
        || a == 100 && (64..=127).contains(&b)
        || a == 192 && b == 0 && c == 0
        || a == 198 && (b == 18 || b == 19)
        || a >= 240)
}

fn is_public_v6(v6: &Ipv6Addr) -> bool {
    let [a, b, ..] = v6.segments();
    !(v6.is_loopback()           // ::1
        || v6.is_unspecified()   // ::
        || v6.is_unique_local()  // fc00::/7 (ULA)
        || v6.is_unicast_link_local() // fe80::/10
        || v6.is_multicast()     // ff00::/8
        // 2001:db8::/32 documentation (`is_documentation` still nightly).
        || a == 0x2001 && b == 0x0db8)
}

/// DNS resolver enforcing the egress guard: resolves through
/// `tokio::net::lookup_host` (same `getaddrinfo` path as the default
/// resolver) and fails the whole connect when *any* returned address is
/// private/reserved — a public name may not carry a private answer. Every
/// hop in the redirect chain resolves through here, and the dial uses
/// exactly these addresses, so the check cannot be raced by
/// re-resolution.
#[derive(Debug, Clone)]
struct GuardedResolver {
    /// `[archive] allow_private`: skip the range check (local indexing).
    allow_private: bool,
}

/// `reqwest::dns::Resolving`'s boxed error — `reqwest::error::BoxError`
/// is not exported, so spell it out (same alias).
type BoxError = Box<dyn std::error::Error + Send + Sync>;

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_private = self.allow_private;
        // A literal IPv6 host can arrive bracketed.
        let host = name
            .as_str()
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or_else(|| name.as_str())
            .to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| Box::new(e) as BoxError)?
                .collect();
            if !allow_private && let Some(bad) = addrs.iter().find(|a| !is_allowed_ip(a.ip())) {
                return Err(Box::new(EgressBlocked {
                    host,
                    addr: bad.ip(),
                }) as BoxError);
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// The `http`/`https` scheme allowlist — unconditional, even under
/// `allow_private` (the opt-in widens the address set, never the
/// schemes: `file:`/`gopher:` reads stay refused).
fn check_scheme(u: &Url) -> Result<(), FetchError> {
    if matches!(u.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(FetchError::Blocked(format!(
            "scheme {:?} is not http or https",
            u.scheme()
        )))
    }
}

/// Per-hop URL validation: the scheme allowlist plus the literal-IP
/// blocklist. A literal IP host never reaches a DNS resolver — hyper
/// dials it directly — so it must be checked here on every hop. DNS
/// names return `Ok` and are validated inside [`GuardedResolver`] at
/// connect time.
fn check_url(u: &Url) -> Result<(), FetchError> {
    check_scheme(u)?;
    let ip = match u.host() {
        Some(Host::Ipv4(v4)) => IpAddr::V4(v4),
        Some(Host::Ipv6(v6)) => IpAddr::V6(v6),
        Some(Host::Domain(_)) => return Ok(()),
        None => return Err(FetchError::Blocked(format!("url {u:?} has no host"))),
    };
    if !is_allowed_ip(ip) {
        return Err(FetchError::Blocked(format!(
            "host {ip} is a private/reserved address"
        )));
    }
    Ok(())
}

/// The `Location` target of a `3xx` response, joined against the page it
/// came from (relative `Location` headers stay valid). `None` when the
/// response is not a redirect or the header is unreadable — the `3xx`
/// itself is returned and the caller maps its status.
fn redirect_target(res: &reqwest::Response, base: &Url) -> Option<Result<Url, FetchError>> {
    if !res.status().is_redirection() {
        return None;
    }
    let loc = res
        .headers()
        .get(reqwest::header::LOCATION)?
        .to_str()
        .ok()?;
    Some(
        base.join(loc)
            .map_err(|e| EngineError::Parse(format!("bad redirect location {loc:?}: {e}")).into()),
    )
}

/// Recover an [`EgressBlocked`] from a `reqwest` error's `source()` chain
/// (reqwest → hyper `ConnectError` → the resolver's `BoxError`), else map
/// the failure with the shared reqwest mapping.
fn map_fetch_error(e: reqwest::Error) -> FetchError {
    let mut cur = std::error::Error::source(&e);
    while let Some(cause) = cur {
        if let Some(b) = cause.downcast_ref::<EgressBlocked>() {
            return FetchError::Blocked(b.to_string());
        }
        cur = cause.source();
    }
    map_reqwest_error(e).into()
}

type HostBucket = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// HTTP fetcher for the archive pipeline. Cheap to clone: connection pool
/// and the host-keyed bucket are shared.
#[derive(Debug, Clone)]
pub struct Fetcher {
    client: reqwest::Client,
    limiter: Arc<HostBucket>,
    /// `[archive] allow_private`: skip the address-range checks (the
    /// resolver carries its own copy; the scheme allowlist stays on).
    allow_private: bool,
}

impl Fetcher {
    /// Build the fetcher with `crate::http` politeness defaults and a
    /// host-keyed token bucket of `requests_per_second`/`burst`.
    /// `allow_private` skips the private/reserved-address egress guard
    /// (`[archive] allow_private`).
    pub fn new(
        requests_per_second: u32,
        burst: u32,
        allow_private: bool,
    ) -> Result<Self, EngineError> {
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
            .dns_resolver(GuardedResolver { allow_private })
            // Redirects are unrolled in `send_chain`: the built-in
            // follower cannot re-validate each hop's URL (and literal-IP
            // hops never reach a resolver at all).
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(4)
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| EngineError::Transport(format!("http client build failed: {e}")))?;

        Ok(Self {
            client,
            limiter: Arc::new(RateLimiter::keyed(
                Quota::per_second(rps).allow_burst(burst),
            )),
            allow_private,
        })
    }

    /// `GET url`: waits on the host's token bucket, then performs the
    /// request with [`FETCH_TIMEOUT`] as the whole-request timeout. The
    /// body streams and is cut at [`MAX_FETCH_BYTES`]; `truncated` reports
    /// the cut.
    ///
    /// Errors: the egress guard to [`FetchError::Blocked`] (scheme
    /// outside `http`/`https`, or a resolved private/reserved address on
    /// any hop); timeouts to `Timeout` and transport failures to
    /// `Transport` under `Engine`, like [`crate::http::HttpClient`].
    /// Non-2xx statuses return `Ok` — status-to-error mapping is the
    /// caller's contract.
    pub async fn get(&self, url: &str) -> Result<FetchedPage, FetchError> {
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

    async fn send(&self, url: &str) -> Result<FetchedPage, FetchError> {
        tokio::time::timeout(FETCH_TIMEOUT, self.send_chain(url))
            .await
            .map_err(|_| FetchError::Engine(EngineError::Timeout))?
    }

    /// [`check_url`] honoring `allow_private`: the opt-in skips the
    /// address-range check the same way it disarms the resolver — the
    /// scheme allowlist always applies.
    fn check_url(&self, u: &Url) -> Result<(), FetchError> {
        if self.allow_private {
            return check_scheme(u);
        }
        check_url(u)
    }

    /// `send` with the redirect chain unrolled: every hop passes
    /// [`check_url`] before it is dialed. The chain shares one
    /// [`FETCH_TIMEOUT`]; past [`MAX_REDIRECTS`] hops the last `3xx`
    /// response is returned like `crate::http::HttpClient` does, so the
    /// caller maps its status.
    async fn send_chain(&self, url: &str) -> Result<FetchedPage, FetchError> {
        let mut current = Url::parse(url)
            .map_err(|e| EngineError::Transport(format!("invalid url {url:?}: {e}")))?;
        self.check_url(&current)?;

        let mut hops = 0;
        let res = loop {
            let res = self
                .client
                .get(current.clone())
                .send()
                .await
                .map_err(map_fetch_error)?;
            match (hops < MAX_REDIRECTS, redirect_target(&res, &current)) {
                (true, Some(Ok(next))) => {
                    self.check_url(&next)?;
                    hops += 1;
                    current = next;
                }
                (true, Some(Err(e))) => return Err(e),
                _ => break res,
            }
        };

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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// The blocklist table: every private/reserved range the guard
    /// rejects, plus the boundaries just outside each one that must stay
    /// reachable.
    #[test]
    fn is_allowed_ip_table() {
        let blocked = [
            // v4 loopback / unspecified / "this network".
            "127.0.0.1",
            "127.53.0.9",
            "0.0.0.0",
            "0.1.2.3",
            // v4 RFC 1918.
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.1",
            // v4 link-local (incl. the cloud metadata endpoint).
            "169.254.0.1",
            "169.254.169.254",
            // CGNAT 100.64/10.
            "100.64.0.1",
            "100.127.255.254",
            // v4 multicast, reserved, broadcast, IETF assignments,
            // benchmarking, documentation.
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.9",
            "198.18.0.1",
            "198.19.255.255",
            // v6: unspecified, loopback, ULA, link-local, multicast,
            // documentation.
            "::",
            "::1",
            "fc00::1",
            "fd00::1",
            "fe80::1",
            "febf::ffff",
            "ff00::1",
            "ff02::1",
            "2001:db8::1",
            // IPv4-mapped IPv6 follows the v4 rules.
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ];
        for ip in blocked {
            assert!(!is_allowed_ip(ip.parse().unwrap()), "{ip} must be blocked");
        }

        let allowed = [
            "8.8.8.8",
            "1.1.1.1",
            // Just outside each blocked v4 range.
            "9.255.255.255",
            "11.0.0.1",
            "172.15.255.1",
            "172.32.0.1",
            "192.169.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "169.253.255.255",
            "223.255.255.255",
            "192.0.1.1",
            "198.17.255.255",
            "198.20.0.1",
            "2001:4860:4860::8888",
            "2606:4700:4700::1111",
            // A mapped public v4 stays allowed.
            "::ffff:8.8.8.8",
        ];
        for ip in allowed {
            assert!(is_allowed_ip(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }

    /// The resolver is where the guard meets the network: a name that
    /// resolves to any private address fails with `EgressBlocked`, and
    /// `allow_private` opts the check out. Every redirect hop resolves
    /// through this same function, so hop coverage follows by
    /// construction.
    #[tokio::test]
    async fn resolver_blocks_private_names() {
        let resolver = GuardedResolver {
            allow_private: false,
        };
        for name in ["127.0.0.1", "10.0.0.1", "localhost"] {
            let err = resolver
                .resolve(Name::from_str(name).unwrap())
                .await
                .err()
                .expect("resolve must fail");
            assert!(
                err.downcast_ref::<EgressBlocked>().is_some(),
                "{name}: expected EgressBlocked, got {err}"
            );
        }

        // `localhost` is a real lookup: it answers loopback and is
        // rejected even though no literal IP was typed.
        let open = GuardedResolver {
            allow_private: true,
        };
        for name in ["127.0.0.1", "localhost"] {
            assert!(
                open.resolve(Name::from_str(name).unwrap()).await.is_ok(),
                "{name}: allow_private must skip the check"
            );
        }
    }
}
