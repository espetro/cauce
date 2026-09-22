//! `Replay`: the deterministic mock engine (parent plan 4.3).
//!
//! Lookup order for a `search` call:
//!
//! 1. Fault injection (`blocked`, `fail_every`, `latency_ms`, `page_limit`,
//!    `empty`), configured via `ReplayOpts` or the `CAUCE_REPLAY_*` env vars.
//! 2. Cassette mode (page 1 only): `<fixtures_root>/<engine>/<sha8>.json`
//!    written by `cauce record` (see [`crate::record`]). With
//!    `cassette_engine` set only that directory is consulted; otherwise every
//!    `<engine>/` subdirectory of `fixtures_root` is scanned in sorted order.
//! 3. Synthetic mode: 10 deterministic results generated from a seeded RNG
//!    (seed = sha256 of the normalized query, mixed with the page), so the
//!    same query always yields the same page with unique normalized URLs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use cauce_core::{
    Engine, EngineError, EngineId, EnginePhase, Metrics, SearchRequest, SearchResult, Tier,
    normalize_query,
};

use crate::cassette::{Cassette, cassette_key};

/// Configuration of a [`Replay`] engine instance.
///
/// `fail_every = 0` disables call failure injection; `page_limit = None`
/// accepts every page. `cassette_engine = None` scans every `<engine>/`
/// subdirectory of `fixtures_root`.
#[derive(Debug, Clone)]
pub struct ReplayOpts {
    /// Root of the cassette tree (`engines/fixtures` in the repo).
    pub fixtures_root: PathBuf,
    /// Restrict cassette lookup to this engine's directory.
    pub cassette_engine: Option<EngineId>,
    /// Sleep this long before responding.
    pub latency_ms: u64,
    /// Every nth call fails with `EngineError::Transport` (0 disables).
    pub fail_every: u64,
    /// Always fail with `EngineError::Blocked`.
    pub blocked: bool,
    /// Always succeed with zero results.
    pub empty: bool,
    /// Highest supported `req.page`; beyond it the call returns `NoResults`.
    pub page_limit: Option<u8>,
}

impl Default for ReplayOpts {
    fn default() -> Self {
        Self {
            fixtures_root: PathBuf::from("engines/fixtures"),
            cassette_engine: None,
            latency_ms: 0,
            fail_every: 0,
            blocked: false,
            empty: false,
            page_limit: None,
        }
    }
}

/// Deterministic replay/synthetic engine. `id = "replay"`, tier 1,
/// `page_size = 10`. Call counting for `fail_every` is per instance and
/// thread-safe (`AtomicU64`).
pub struct Replay {
    opts: ReplayOpts,
    calls: AtomicU64,
    /// W1-09 phase timings (`cauce_engine_duration_ms{phase}`). Unbound until
    /// the first record, which resolves the process-global meter provider.
    metrics: Metrics,
}

impl Replay {
    pub fn new(opts: ReplayOpts) -> Self {
        Self {
            opts,
            calls: AtomicU64::new(0),
            metrics: Metrics,
        }
    }

    /// Build from the `CAUCE_REPLAY_*` environment:
    /// `CAUCE_REPLAY_LATENCY_MS`, `CAUCE_REPLAY_FAIL_EVERY`, `CAUCE_REPLAY_BLOCKED`,
    /// `CAUCE_REPLAY_EMPTY`, `CAUCE_REPLAY_PAGE_LIMIT`, plus the W0-local
    /// `CAUCE_REPLAY_FIXTURES_DIR` and `CAUCE_REPLAY_CASSETTE_ENGINE`.
    pub fn from_env() -> Self {
        let mut opts = ReplayOpts::default();
        if let Some(dir) = env_opt("CAUCE_REPLAY_FIXTURES_DIR") {
            opts.fixtures_root = PathBuf::from(dir);
        }
        if let Some(engine) = env_opt("CAUCE_REPLAY_CASSETTE_ENGINE") {
            opts.cassette_engine = Some(EngineId::from(engine));
        }
        if let Some(v) = env_opt("CAUCE_REPLAY_LATENCY_MS").and_then(|v| v.parse().ok()) {
            opts.latency_ms = v;
        }
        if let Some(v) = env_opt("CAUCE_REPLAY_FAIL_EVERY").and_then(|v| v.parse().ok()) {
            opts.fail_every = v;
        }
        if let Some(v) = env_opt("CAUCE_REPLAY_BLOCKED") {
            opts.blocked = env_truthy(&v);
        }
        if let Some(v) = env_opt("CAUCE_REPLAY_EMPTY") {
            opts.empty = env_truthy(&v);
        }
        if let Some(v) = env_opt("CAUCE_REPLAY_PAGE_LIMIT").and_then(|v| v.parse().ok()) {
            opts.page_limit = Some(v);
        }
        Self::new(opts)
    }

    /// Number of `search` calls seen by this instance.
    pub fn call_count(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    /// The options this instance was built with.
    pub fn opts(&self) -> &ReplayOpts {
        &self.opts
    }

    /// Path of the cassette for `query`, if one exists on disk.
    fn cassette_file(&self, query: &str) -> Option<PathBuf> {
        let file = format!("{}.json", cassette_key(query));
        if let Some(engine) = &self.opts.cassette_engine {
            let path = self.opts.fixtures_root.join(engine.as_str()).join(&file);
            return path.is_file().then_some(path);
        }
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.opts.fixtures_root)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort_unstable();
        dirs.into_iter()
            .map(|d| d.join(&file))
            .find(|p| p.is_file())
    }
}

fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[async_trait]
impl Engine for Replay {
    fn id(&self) -> EngineId {
        EngineId::from("replay")
    }

    fn tier(&self) -> Tier {
        Tier::T1
    }

    fn page_size(&self) -> u8 {
        synth::PAGE_SIZE
    }

    async fn search(
        &self,
        req: &SearchRequest,
        _budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;

        if self.opts.blocked {
            return Err(EngineError::Blocked);
        }
        // `is_multiple_of(0)` is false for call >= 1, so 0 disables injection.
        if call.is_multiple_of(self.opts.fail_every) {
            return Err(EngineError::Transport(format!(
                "injected failure (call {call})"
            )));
        }
        // `phase=http` covers replay's fetch leg: the simulated upstream
        // latency plus the cassette filesystem probe/read. `phase=parse`
        // covers turning bytes (or the seeded RNG) into `SearchResult`s.
        let fetch = Instant::now();
        if self.opts.latency_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.opts.latency_ms)).await;
        }
        if let Some(limit) = self.opts.page_limit
            && req.page > limit
        {
            self.metrics
                .record_engine_phase(&self.id(), EnginePhase::Http, fetch.elapsed());
            return Err(EngineError::NoResults);
        }
        if self.opts.empty {
            self.metrics
                .record_engine_phase(&self.id(), EnginePhase::Http, fetch.elapsed());
            return Ok(Vec::new());
        }

        if req.page == 1
            && let Some(path) = self.cassette_file(&req.q)
        {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| EngineError::Transport(format!("{}: {e}", path.display())))?;
            self.metrics
                .record_engine_phase(&self.id(), EnginePhase::Http, fetch.elapsed());
            let parse = Instant::now();
            let cassette: Cassette = serde_json::from_str(&text)
                .map_err(|e| EngineError::Parse(format!("{}: {e}", path.display())))?;
            self.metrics
                .record_engine_phase(&self.id(), EnginePhase::Parse, parse.elapsed());
            return Ok(cassette.results);
        }

        self.metrics
            .record_engine_phase(&self.id(), EnginePhase::Http, fetch.elapsed());
        let parse = Instant::now();
        let results = synth::results(&normalize_query(&req.q), req.page, &self.id());
        self.metrics
            .record_engine_phase(&self.id(), EnginePhase::Parse, parse.elapsed());
        Ok(results)
    }
}

/// Synthetic result generation, seeded by `sha256(normalized_query)` mixed
/// with the page number. Pure functions of the seed: stable across runs,
/// platforms and Rust versions (a fixed splitmix64, not `rand::StdRng`).
mod synth {
    use std::collections::HashSet;

    use sha2::{Digest, Sha256};
    use url::Url;

    use cauce_core::{EngineId, SearchResult, normalize_url};

    /// Results per page, matching `Replay::page_size`.
    pub const PAGE_SIZE: u8 = 10;

    const HOSTS: &[&str] = &[
        "devdocs.io",
        "github.com",
        "developer.mozilla.org",
        "news.ycombinator.com",
        "stackoverflow.com",
        "docs.rs",
        "crates.io",
        "blog.rust-lang.org",
        "en.wikipedia.org",
        "arxiv.org",
        "lobste.rs",
        "freecodecamp.org",
    ];

    const TOPICS: &[&str] = &[
        "guide",
        "reference",
        "tutorial",
        "handbook",
        "overview",
        "cookbook",
        "patterns",
        "internals",
        "faq",
        "notes",
    ];

    const ADJECTIVES: &[&str] = &[
        "practical",
        "complete",
        "gentle",
        "modern",
        "opinionated",
        "minimal",
        "advanced",
        "idiomatic",
        "hands-on",
        "definitive",
    ];

    const NOUNS: &[&str] = &[
        "examples",
        "trade-offs",
        "pitfalls",
        "recipes",
        "benchmarks",
        "checklists",
        "workflows",
        "case studies",
        "tips",
        "deep dives",
    ];

    /// Splitmix64: a tiny deterministic PRNG. Not for cryptography.
    struct Splitmix64(u64);

    impl Splitmix64 {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
            items[(self.next_u64() % items.len() as u64) as usize]
        }
    }

    fn seed(normalized_query: &str, page: u8) -> u64 {
        let digest = Sha256::digest(normalized_query.as_bytes());
        let base = u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"));
        base ^ (page as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// `PAGE_SIZE` plausible results for one page of `normalized_query`.
    /// URLs carry the result index in the path, so all normalized URLs on a
    /// page are distinct.
    pub fn results(normalized_query: &str, page: u8, engine: &EngineId) -> Vec<SearchResult> {
        let mut rng = Splitmix64(seed(normalized_query, page));
        let slug = if normalized_query.is_empty() {
            "results".to_string()
        } else {
            normalized_query.replace(' ', "-")
        };
        let title_query = if normalized_query.is_empty() {
            "results".to_string()
        } else {
            normalized_query.to_string()
        };

        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(PAGE_SIZE as usize);
        for i in 0..PAGE_SIZE {
            let host = rng.pick(HOSTS);
            let topic = rng.pick(TOPICS);
            let adjective = rng.pick(ADJECTIVES);
            let noun = rng.pick(NOUNS);
            let url = normalize_url(
                &Url::parse(&format!("https://{host}/{topic}/{slug}-{i}"))
                    .expect("generated url parses"),
            );
            if !seen.insert(url.as_str().to_string()) {
                continue;
            }
            let article = if adjective.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            out.push(SearchResult {
                url,
                title: format!("{title_query}: {article} {adjective} {topic} with {noun}"),
                snippet: format!(
                    "{}{} {adjective} {topic} on {title_query}. Covers {noun}, common \
                     pitfalls and practical examples. Page {page} result {i}.",
                    article[..1].to_uppercase(),
                    &article[1..],
                ),
                engine: engine.clone(),
                published: None,
                score: 1.0 / f32::from(i + 1),
            });
        }
        out
    }
}
