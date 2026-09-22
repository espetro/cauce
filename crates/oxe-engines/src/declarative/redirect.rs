//! `unwrap_redirect` rules: tracking-redirect unwrapping for declarative
//! specs (W1-02). Two extract shapes cover the known wrappers:
//!
//! ```yaml
//! unwrap_redirect:
//!   # https://www.bing.com/ck/a?...&u=a1<base64url(target)>&ntb=1
//!   - match: "bing.com/ck/a"
//!     param: u            # take this query parameter
//!     strip: "a1"         # drop a literal prefix first (optional)
//!     base64: true        # then base64url-decode (optional)
//!   # https://r.search.yahoo.com/_ylt=../RU=https%3a%2f%2ftarget%2f/RK=..
//!   - match: "r.search.yahoo.com"
//!     between: { start: "/RU=", end: "/" }   # percent-decoded path segment
//! ```
//!
//! `match` is a host suffix plus an optional path prefix:
//! `"bing.com/ck/a"` matches `www.bing.com` + `/ck/a...`. A rule that
//! matches but fails to extract (missing param, undecodable payload,
//! non-URL result) leaves the original URL untouched — the rule list is
//! tried in order and the first successful unwrap wins.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use serde::Deserialize;
use url::Url;

/// Raw `unwrap_redirect` list entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedirectRule {
    /// Host suffix plus optional path prefix (`"bing.com/ck/a"`).
    #[serde(rename = "match")]
    pub match_: String,
    /// Extract this query parameter's value.
    #[serde(default)]
    pub param: Option<String>,
    /// Literal prefix to strip from the extracted value (`a1` for Bing).
    #[serde(default)]
    pub strip: Option<String>,
    /// Base64-decode the value after stripping (url-safe alphabet first).
    #[serde(default)]
    pub base64: bool,
    /// Extract the path segment between `start` and `end`, percent-decoded.
    #[serde(default)]
    pub between: Option<BetweenMarkers>,
}

/// `between:` markers for path-segment extraction.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BetweenMarkers {
    /// Segment start marker (`/RU=` for Yahoo).
    pub start: String,
    /// Segment end marker (`/` for Yahoo).
    pub end: String,
}

/// A rule with its `match` pre-split into host suffix + optional path
/// prefix. Compiled at spec load.
#[derive(Debug)]
pub(crate) struct CompiledRedirect {
    /// Required host suffix (`bing.com` matches `www.bing.com`).
    host_suffix: String,
    /// Optional required path prefix.
    path_prefix: Option<String>,
    param: Option<String>,
    strip: Option<String>,
    base64: bool,
    between: Option<BetweenMarkers>,
}

impl RedirectRule {
    /// Validate and split `match`; exactly one of `param`/`between` must be
    /// set.
    pub(crate) fn compile(&self) -> Result<CompiledRedirect, String> {
        if self.param.is_some() == self.between.is_some() {
            return Err(format!(
                "unwrap_redirect rule {:?}: exactly one of `param` or `between` is required",
                self.match_
            ));
        }
        let (host_suffix, path_prefix) = match self.match_.split_once('/') {
            Some((host, path)) => (host.to_string(), Some(format!("/{path}"))),
            None => (self.match_.clone(), None),
        };
        if host_suffix.is_empty() {
            return Err(format!(
                "unwrap_redirect rule {:?}: empty host in `match`",
                self.match_
            ));
        }
        Ok(CompiledRedirect {
            host_suffix,
            path_prefix,
            param: self.param.clone(),
            strip: self.strip.clone(),
            base64: self.base64,
            between: self.between.clone(),
        })
    }
}

impl CompiledRedirect {
    fn matches(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        let host_ok = host == self.host_suffix || host.ends_with(&format!(".{}", self.host_suffix));
        if !host_ok {
            return false;
        }
        match &self.path_prefix {
            Some(prefix) => url.path().starts_with(prefix.as_str()),
            None => true,
        }
    }

    fn extract(&self, url: &Url) -> Option<Url> {
        if let Some(param) = &self.param {
            let raw = url
                .query_pairs()
                .find(|(k, _)| k == param.as_str())?
                .1
                .into_owned();
            let stripped = match &self.strip {
                Some(prefix) => raw
                    .strip_prefix(prefix.as_str())
                    .unwrap_or(&raw)
                    .to_string(),
                None => raw,
            };
            // `query_pairs` already percent-decoded the value; decoding
            // again would corrupt payloads containing literal `%`.
            let decoded = if self.base64 {
                let bytes = [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD_NO_PAD, &STANDARD]
                    .into_iter()
                    .find_map(|engine| engine.decode(stripped.as_bytes()).ok())?;
                String::from_utf8(bytes).ok()?
            } else {
                stripped
            };
            return Url::parse(decoded.trim()).ok();
        }
        if let Some(between) = &self.between {
            let path = url.path();
            let start = path.find(&between.start)? + between.start.len();
            let tail = &path[start..];
            let end = tail.find(&between.end).unwrap_or(tail.len());
            let decoded = percent_decode(&tail[..end])?;
            return Url::parse(decoded.trim()).ok();
        }
        None
    }
}

/// Percent-decode a path segment or parameter value (UTF-8 required —
/// a decoded payload that is not UTF-8 cannot be a URL).
fn percent_decode(raw: &str) -> Option<String> {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()
        .map(|s| s.into_owned())
}

/// Unwrap `url` through `rules`; returns the original when no rule both
/// matches and extracts.
pub(crate) fn unwrap_redirect(url: &Url, rules: &[CompiledRedirect]) -> Url {
    for rule in rules {
        if rule.matches(url)
            && let Some(unwrapped) = rule.extract(url)
        {
            return unwrapped;
        }
    }
    url.clone()
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::*;

    fn rule(yaml: &str) -> CompiledRedirect {
        let raw: RedirectRule = serde_norway::from_str(yaml).unwrap();
        raw.compile().unwrap()
    }

    #[test]
    fn bing_ck_a_unwraps_a1_base64() {
        let target = "https://tanstack.com/router/latest";
        let u = format!("a1{}", URL_SAFE_NO_PAD.encode(target));
        let url = Url::parse(&format!("https://www.bing.com/ck/a?!&&p=abc&u={u}&ntb=1")).unwrap();
        let rules = vec![rule(
            r#"{ match: "bing.com/ck/a", param: u, strip: "a1", base64: true }"#,
        )];
        assert_eq!(unwrap_redirect(&url, &rules).as_str(), target);
    }

    #[test]
    fn bing_rule_ignores_other_hosts_and_paths() {
        let rules = vec![rule(
            r#"{ match: "bing.com/ck/a", param: u, strip: "a1", base64: true }"#,
        )];
        for raw in [
            "https://www.bing.com/search?q=x",
            "https://bing.com.evil.example/ck/a?u=a1aaaa",
            "https://example.com/?u=a1aaaa",
        ] {
            let url = Url::parse(raw).unwrap();
            assert_eq!(unwrap_redirect(&url, &rules), url);
        }
    }

    #[test]
    fn yahoo_ru_segment_unwraps() {
        let url = Url::parse(
            "https://r.search.yahoo.com/_ylt=Awr/RU=https%3a%2f%2fexample.com%2fdocs%3fx%3d1/RK=2/RS=abc",
        )
        .unwrap();
        let rules = vec![rule(
            r#"{ match: "r.search.yahoo.com", between: { start: "/RU=", end: "/" } }"#,
        )];
        assert_eq!(
            unwrap_redirect(&url, &rules).as_str(),
            "https://example.com/docs?x=1"
        );
    }

    #[test]
    fn failed_extraction_keeps_original() {
        let rules = vec![rule(
            r#"{ match: "bing.com/ck/a", param: u, strip: "a1", base64: true }"#,
        )];
        // Matching host+path but `u` decodes to garbage.
        let url = Url::parse("https://www.bing.com/ck/a?u=a1%%%not-base64").unwrap();
        assert_eq!(unwrap_redirect(&url, &rules), url);
    }

    #[test]
    fn rule_requires_exactly_one_extract_shape() {
        assert!(
            serde_norway::from_str::<RedirectRule>(r#"{ match: "x" }"#)
                .unwrap()
                .compile()
                .is_err()
        );
        assert!(
            serde_norway::from_str::<RedirectRule>(
                r#"{ match: "x", param: u, between: { start: "/", end: "/" } }"#
            )
            .unwrap()
            .compile()
            .is_err()
        );
    }
}
