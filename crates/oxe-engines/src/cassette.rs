//! Replay cassette format: one JSON file per (engine, normalized query),
//! stored at `<fixtures_root>/<engine>/<sha8(normalized_query)>.json`.
//!
//! `sha8` is the first 8 hex chars of the sha256 digest of the normalized
//! query (`normalize_query`), so a cassette key never leaks the raw query and
//! is stable across filesystems.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use oxe_core::{EngineId, SearchResult, normalize_query};

/// `sha8`: first 8 hex chars of sha256 over the normalized form of `query`.
pub fn cassette_key(query: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(normalize_query(query).as_bytes()));
    digest[..8].to_owned()
}

/// On-disk path of the cassette for `engine` + `query`:
/// `<fixtures_root>/<engine>/<sha8>.json`.
pub fn cassette_path(fixtures_root: &Path, engine: &str, query: &str) -> PathBuf {
    fixtures_root
        .join(engine)
        .join(format!("{}.json", cassette_key(query)))
}

/// Errors reading or writing a cassette file.
#[derive(Debug, thiserror::Error)]
pub enum CassetteError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid cassette json: {0}")]
    Json(#[from] serde_json::Error),
}

/// One recorded engine page (parent plan 4.3). `raw_response_path` is
/// optional: `Engine::search` only yields parsed results, so cassettes
/// recorded through the trait leave it empty; hand-authored fixtures may point
/// at a sibling file holding the raw upstream body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cassette {
    /// The query as recorded (normalized form, matching the filename key).
    pub query: String,
    /// Engine that produced the results (`bing`, `ddgs`, ...).
    pub engine: EngineId,
    pub recorded_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_response_path: Option<PathBuf>,
    pub results: Vec<SearchResult>,
}

impl Cassette {
    pub fn new(engine: EngineId, query: &str, results: Vec<SearchResult>) -> Self {
        Self {
            query: normalize_query(query),
            engine,
            recorded_at: Utc::now(),
            raw_response_path: None,
            results,
        }
    }

    pub fn load(path: &Path) -> Result<Self, CassetteError> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Write pretty JSON with a trailing newline; creates `<engine>/` when
    /// missing.
    pub fn save(&self, path: &Path) -> Result<(), CassetteError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, format!("{json}\n"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_sha8_of_normalized_query() {
        // "  Foo\tBAR " normalizes to "foo bar".
        let expected = format!("{:x}", Sha256::digest(b"foo bar"));
        assert_eq!(cassette_key("  Foo\tBAR "), expected[..8]);
        assert_eq!(cassette_key("foo bar"), cassette_key("  FOO  bar "));
        assert_ne!(cassette_key("foo bar"), cassette_key("foo baz"));
    }

    #[test]
    fn path_layout() {
        let p = cassette_path(Path::new("engines/fixtures"), "ddgs", "foo bar");
        assert_eq!(
            p,
            PathBuf::from("engines/fixtures")
                .join("ddgs")
                .join(format!("{}.json", cassette_key("foo bar")))
        );
    }
}
