//! URL normalisation for merge dedupe and result identity.
//!
//! `normalize_url` produces the canonical form used to dedupe results across
//! engines (parent plan 4.4 step 5): lowercase scheme and host, tracking
//! parameters stripped (`utm_*`, `fbclid`, `gclid`), remaining query pairs
//! sorted by key then value, default ports removed, fragment dropped,
//! trailing slashes removed from the path. The function is idempotent.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use url::Url;

/// True when `key` is a click/tracking parameter that never changes page
/// content: `utm_*` (GA), `fbclid` (Meta), `gclid` (Google Ads). Matched
/// case-insensitively (`UTM_SOURCE` is stripped too).
fn is_tracking_param(key: &str) -> bool {
    let k = key.to_lowercase();
    k.starts_with("utm_") || k == "fbclid" || k == "gclid"
}

/// Canonical form of `url` for dedupe. See module docs for the rule list.
/// Idempotent: `normalize_url(&normalize_url(u)) == normalize_url(u)`.
pub fn normalize_url(url: &Url) -> Url {
    let mut u = url.clone();

    // Url::parse already lowercases scheme and host; enforce for Urls built
    // programmatically.
    let _ = u.set_scheme(&u.scheme().to_lowercase());
    if let Some(host) = u.host_str() {
        let lower = host.to_lowercase();
        if lower != host {
            let _ = u.set_host(Some(&lower));
        }
    }

    // Drop the port when it is the default for the scheme (Url::parse already
    // does this for the special schemes; keep it explicit for the rest).
    let is_default_port = matches!(
        (u.scheme(), u.port()),
        ("http" | "ws", Some(80)) | ("https" | "wss", Some(443)) | ("ftp", Some(21))
    );
    if is_default_port {
        let _ = u.set_port(None);
    }

    // Strip tracking params, sort the rest for a canonical form.
    let mut kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    kept.sort();
    {
        let mut qp = u.query_pairs_mut();
        qp.clear();
        qp.extend_pairs(kept.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    if u.query() == Some("") {
        u.set_query(None);
    }

    // Fragments never change the fetched page.
    u.set_fragment(None);

    // Strip trailing slashes; the root path "/" is left alone (it cannot be
    // removed from a special-scheme URL anyway).
    if u.path().len() > 1 && u.path().ends_with('/') {
        let p = u.path().trim_end_matches('/').to_string();
        u.set_path(&p);
    }

    u
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn strips_tracking_params_and_keeps_real_ones_sorted() {
        let u = Url::parse(
            "https://example.com/p?b=2&utm_source=x&a=1&fbclid=y&gclid=z&utm_medium=m&UTM_CAMPAIGN=C&FBCLID=Y",
        )
        .unwrap();
        let n = normalize_url(&u);
        assert_eq!(n.as_str(), "https://example.com/p?a=1&b=2");
    }

    #[test]
    fn lowercases_scheme_and_host_strips_default_port_fragment_trailing_slash() {
        let u = Url::parse("HTTP://EXAMPLE.com:80/a/b/#frag").unwrap();
        let n = normalize_url(&u);
        assert_eq!(n.as_str(), "http://example.com/a/b");
    }

    #[test]
    fn keeps_non_default_port_and_root_slash() {
        let u = Url::parse("https://example.com:8443/").unwrap();
        let n = normalize_url(&u);
        assert_eq!(n.as_str(), "https://example.com:8443/");
    }

    #[test]
    fn empty_query_after_stripping_is_removed() {
        let u = Url::parse("https://example.com/?utm_campaign=c").unwrap();
        let n = normalize_url(&u);
        assert_eq!(n.as_str(), "https://example.com/");
        assert_eq!(n.query(), None);
    }

    fn tracking_key() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec![
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "UTM_SOURCE",
            "Utm_Term",
            "fbclid",
            "gclid",
            "FBCLID",
            "a",
            "q",
            "x",
        ])
    }

    fn arb_url() -> impl Strategy<Value = Url> {
        (
            prop::sample::select(vec!["http", "https"]),
            "[a-z][a-z0-9]{1,9}(\\.[a-z][a-z0-9]{1,7}){1,2}",
            prop::option::of(prop::sample::select(vec![80u16, 443, 8080, 3000])),
            prop::collection::vec("[a-z0-9_-]{1,10}", 0..4),
            prop::collection::vec((tracking_key(), "[a-z0-9]{0,10}"), 0..5),
            prop::option::of("[a-z0-9]{1,8}"),
        )
            .prop_map(|(scheme, host, port, segs, pairs, frag)| {
                let mut s = format!("{scheme}://{host}");
                if let Some(p) = port {
                    s.push_str(&format!(":{p}"));
                }
                for seg in segs {
                    s.push('/');
                    s.push_str(&seg);
                }
                // Sometimes add a trailing slash.
                if pairs.len() % 2 == 0 {
                    s.push('/');
                }
                if !pairs.is_empty() {
                    s.push('?');
                    s.push_str(
                        &pairs
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join("&"),
                    );
                }
                if let Some(f) = frag {
                    s.push('#');
                    s.push_str(&f);
                }
                Url::parse(&s).expect("generated url parses")
            })
    }

    proptest! {
        #[test]
        fn normalize_url_is_idempotent(u in arb_url()) {
            let once = normalize_url(&u);
            let twice = normalize_url(&once);
            prop_assert_eq!(&once, &twice);
        }

        #[test]
        fn normalize_url_strips_tracking_and_fragment(u in arb_url()) {
            let n = normalize_url(&u);
            prop_assert!(n.fragment().is_none());
            for (k, _) in n.query_pairs() {
                prop_assert!(!is_tracking_param(&k), "tracking param left: {k}");
            }
        }
    }
}
