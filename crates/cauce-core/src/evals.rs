//! Engine relevance evals (W3-05): the shared file formats and scoring for
//! `cauce eval engines`, the `evals/results/<date>-engines.json` artifact the
//! nightly workflow uploads, and the `evals/thresholds.toml` gate the
//! tracking-issue step reads.
//!
//! Case files are JSONL at `evals/engines/*.jsonl`, one object per line
//! (settled inputs):
//!
//! ```json
//! {"query": "tanstack router docs", "expect_domains_top5": ["tanstack.com"], "engines": ["bing"]}
//! ```
//!
//! Scoring is domain-hit@5: a case hits when any expected domain appears in
//! the host of one of the engine's first five [`normalize_url`]-normalized
//! result URLs (a host matches `d` when it equals `d` or is a subdomain of
//! `d`). The nightly workflow's score never gates merges — below-threshold
//! engines only feed the single tracking issue.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EngineId, SearchResult, normalize_url};

/// Results directory: `$CAUCE_EVAL_RESULTS_DIR` or `evals/results` relative
/// to the process cwd — the same place `cauce eval engines` writes and the
/// `/api/stats` handler reads (read per request, never cached state).
pub fn results_dir() -> PathBuf {
    std::env::var("CAUCE_EVAL_RESULTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("evals/results"))
}

/// One case line of an `evals/engines/*.jsonl` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    /// The query to run through each named engine.
    pub query: String,
    /// Accepted result domains; a hit is any of them in the top 5.
    pub expect_domains_top5: Vec<String>,
    /// Engine ids the case runs against (non-empty).
    pub engines: Vec<EngineId>,
}

/// Failures loading cases, thresholds or reports.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("{path}: line {line}: invalid case json: {source}")]
    CaseJson {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path}: line {line}: {reason}")]
    CaseShape {
        path: PathBuf,
        line: usize,
        reason: String,
    },
    #[error("thresholds file {0}: {1}")]
    Thresholds(PathBuf, String),
    #[error("invalid report json {path}: {source}")]
    ReportJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Parse every case in a JSONL file. Blank lines are skipped; malformed or
/// semantically empty lines are errors — a case file that silently drops
/// lines would inflate scores.
pub fn load_cases(path: &Path) -> Result<Vec<EvalCase>, EvalError> {
    let text = std::fs::read_to_string(path)?;
    let mut cases = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let line_no = idx + 1;
        let case: EvalCase = serde_json::from_str(line).map_err(|source| EvalError::CaseJson {
            path: path.to_path_buf(),
            line: line_no,
            source,
        })?;
        if case.query.trim().is_empty() {
            return Err(EvalError::CaseShape {
                path: path.to_path_buf(),
                line: line_no,
                reason: "query is empty".to_string(),
            });
        }
        if case.expect_domains_top5.is_empty() {
            return Err(EvalError::CaseShape {
                path: path.to_path_buf(),
                line: line_no,
                reason: "expect_domains_top5 is empty".to_string(),
            });
        }
        if case.engines.is_empty() {
            return Err(EvalError::CaseShape {
                path: path.to_path_buf(),
                line: line_no,
                reason: "engines is empty".to_string(),
            });
        }
        cases.push(case);
    }
    Ok(cases)
}

/// Whether `host` matches expected domain `domain`: equal, or a subdomain
/// (`www.tanstack.com` matches `tanstack.com`; `faketanstack.com` does not).
pub fn domain_matches(host: &str, domain: &str) -> bool {
    let host = host.trim_end_matches('.').to_lowercase();
    let domain = domain.trim_end_matches('.').to_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Hosts of the first `top_n` results after [`normalize_url`] — the strings
/// the domain check runs on and the report stores for review.
pub fn top_hosts(results: &[SearchResult], top_n: usize) -> Vec<String> {
    results
        .iter()
        .take(top_n)
        .filter_map(|r| normalize_url(&r.url).host_str().map(str::to_string))
        .collect()
}

/// `true` when any expected domain appears in `hosts` (the engine's top-5
/// normalized result hosts).
pub fn domain_hit(hosts: &[String], expected: &[String]) -> bool {
    expected
        .iter()
        .any(|d| hosts.iter().any(|h| domain_matches(h, d)))
}

/// `evals/thresholds.toml`: per-engine minimum domain-hit@5.
///
/// ```toml
/// default = 0.8
///
/// [engines]
/// bing = 0.8
/// ```
///
/// An engine without an entry falls back to `default` (itself defaulting to
/// 0.0 — a file that only names engines still parses).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    #[serde(default)]
    pub default: f64,
    #[serde(default)]
    pub engines: BTreeMap<String, f64>,
}

impl Thresholds {
    pub fn load(path: &Path) -> Result<Self, EvalError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| EvalError::Thresholds(path.to_path_buf(), e.to_string()))?;
        let t: Self = toml::from_str(&text)
            .map_err(|e| EvalError::Thresholds(path.to_path_buf(), e.to_string()))?;
        for (name, v) in t
            .engines
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .chain(std::iter::once(("default", t.default)))
        {
            if !(0.0..=1.0).contains(&v) {
                return Err(EvalError::Thresholds(
                    path.to_path_buf(),
                    format!("{name} = {v}: threshold must be within 0.0..=1.0"),
                ));
            }
        }
        Ok(t)
    }

    /// The threshold applying to `engine`.
    pub fn for_engine(&self, engine: &str) -> f64 {
        self.engines.get(engine).copied().unwrap_or(self.default)
    }
}

/// Per-engine score row of the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineScore {
    pub engine: EngineId,
    /// Cases naming this engine.
    pub cases: u32,
    /// Cases where an expected domain appeared in the top 5.
    pub hits: u32,
    /// `hits / cases` — 0 when `cases` is 0.
    pub domain_hit_at5: f64,
    /// Threshold applied from `evals/thresholds.toml`.
    pub threshold: f64,
    /// `domain_hit_at5 >= threshold`.
    pub ok: bool,
}

/// One case outcome for one engine — the auditable detail behind the
/// aggregate `domain_hit_at5`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseOutcome {
    pub engine: EngineId,
    pub query: String,
    pub expect_domains_top5: Vec<String>,
    /// `hit` is `false` when the engine errored or returned fewer than the
    /// expected domains in its top 5.
    pub hit: bool,
    /// Hosts of the engine's top-5 normalized URLs (may be shorter on error).
    pub top5_hosts: Vec<String>,
    /// Engine error or `no cassette` when the outcome is a miss for a reason
    /// other than returned results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The `evals/results/<date>-engines.json` artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    /// Always `"engines"` — future eval kinds get their own suffix.
    pub kind: String,
    /// `YYYY-MM-DD` — the filename stem.
    pub date: String,
    pub generated_at: DateTime<Utc>,
    /// `true` for a `--live` run (real engines), `false` for replay
    /// cassettes.
    pub live: bool,
    /// Per-engine aggregates, sorted by engine id.
    pub engines: Vec<EngineScore>,
    /// Engine ids below their threshold — feeds the workflow's single
    /// tracking issue.
    pub below_threshold: Vec<EngineId>,
    /// One row per (case, engine) pair.
    pub outcomes: Vec<CaseOutcome>,
}

impl EvalReport {
    /// Stamp `date`/`generated_at` at now (UTC) and assemble the report.
    pub fn new(
        live: bool,
        engines: Vec<EngineScore>,
        below_threshold: Vec<EngineId>,
        outcomes: Vec<CaseOutcome>,
    ) -> Self {
        let now = Utc::now();
        Self {
            kind: "engines".to_string(),
            date: now.format("%Y-%m-%d").to_string(),
            generated_at: now,
            live,
            engines,
            below_threshold,
            outcomes,
        }
    }
}

/// Write `report` to `<dir>/<report.date>-engines.json`; returns the path.
pub fn write_report(dir: &Path, report: &EvalReport) -> Result<PathBuf, EvalError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}-engines.json", report.date));
    let json = serde_json::to_string_pretty(report).expect("EvalReport serializes");
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(path)
}

/// The newest `<date>-engines.json` under `dir` (dates sort lexically),
/// parsed. `Ok(None)` when the directory has none — `/api/stats` then omits
/// the field. A malformed newest file is an error, not a silent skip.
pub fn latest_report(dir: &Path) -> Result<Option<EvalReport>, EvalError> {
    let mut names: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with("-engines.json"))
            })
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    names.sort_unstable();
    let Some(path) = names.pop() else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)?;
    let report: EvalReport =
        serde_json::from_str(&text).map_err(|source| EvalError::ReportJson {
            path: path.clone(),
            source,
        })?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_matches_exact_and_subdomain() {
        assert!(domain_matches("tanstack.com", "tanstack.com"));
        assert!(domain_matches("www.tanstack.com", "tanstack.com"));
        assert!(domain_matches("EN.Wikipedia.org", "wikipedia.ORG"));
        assert!(!domain_matches("faketanstack.com", "tanstack.com"));
        assert!(!domain_matches("tanstack.com.evil.example", "tanstack.com"));
    }

    #[test]
    fn load_cases_parses_jsonl_and_rejects_empty_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cases.jsonl");
        std::fs::write(
            &path,
            "{\"query\": \"q1\", \"expect_domains_top5\": [\"a.com\"], \"engines\": [\"bing\"]}\n\n",
        )
        .unwrap();
        let cases = load_cases(&path).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].query, "q1");
        assert_eq!(cases[0].engines, vec![EngineId::from("bing")]);

        for (line, reason) in [
            (
                "{\"query\": \"\", \"expect_domains_top5\": [\"a.com\"], \"engines\": [\"b\"]}",
                "query is empty",
            ),
            (
                "{\"query\": \"q\", \"expect_domains_top5\": [], \"engines\": [\"b\"]}",
                "expect_domains_top5 is empty",
            ),
            (
                "{\"query\": \"q\", \"expect_domains_top5\": [\"a.com\"], \"engines\": []}",
                "engines is empty",
            ),
        ] {
            std::fs::write(&path, format!("{line}\n")).unwrap();
            let err = load_cases(&path).unwrap_err().to_string();
            assert!(err.contains(reason), "{err} should contain {reason:?}");
        }
    }

    #[test]
    fn thresholds_default_and_per_engine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thresholds.toml");
        std::fs::write(&path, "default = 0.5\n\n[engines]\nbing = 0.9\n").unwrap();
        let t = Thresholds::load(&path).unwrap();
        assert_eq!(t.for_engine("bing"), 0.9);
        assert_eq!(t.for_engine("brave"), 0.5);

        std::fs::write(&path, "default = 1.5\n").unwrap();
        assert!(Thresholds::load(&path).is_err());
    }

    #[test]
    fn write_then_latest_roundtrip_and_latest_picks_newest() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |date: &str| EvalReport {
            kind: "engines".to_string(),
            date: date.to_string(),
            generated_at: Utc::now(),
            live: false,
            engines: vec![],
            below_threshold: vec![],
            outcomes: vec![],
        };
        write_report(dir.path(), &mk("2026-09-23")).unwrap();
        write_report(dir.path(), &mk("2026-09-24")).unwrap();
        let latest = latest_report(dir.path()).unwrap().unwrap();
        assert_eq!(latest.date, "2026-09-24");

        let empty = tempfile::tempdir().unwrap();
        assert!(latest_report(empty.path()).unwrap().is_none());
    }

    #[test]
    fn domain_hit_scores_top5_hosts() {
        let hosts = vec![
            "a.com".to_string(),
            "www.tanstack.com".to_string(),
            "c.com".to_string(),
        ];
        assert!(domain_hit(&hosts, &["tanstack.com".to_string()]));
        assert!(!domain_hit(&hosts, &["d.com".to_string()]));
    }
}
