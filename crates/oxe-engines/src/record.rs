//! `record`: run any `Engine` once for a query and write a replay cassette
//! (see [`crate::cassette`]). Backs `oxe record --engine <id> --query <q>`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};
use std::time::Duration;

use oxe_core::{ClientKind, Engine, EngineError, SafeSearch, SearchRequest};

use crate::cassette::{Cassette, CassetteError, cassette_path};

/// Upstream budget for a single recording call.
const RECORD_BUDGET: Duration = Duration::from_secs(30);

/// Why a recording failed.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    /// The engine call itself failed.
    #[error("engine failed: {0}")]
    Engine(#[from] EngineError),
    /// The cassette could not be written.
    #[error("writing cassette: {0}")]
    Cassette(#[from] CassetteError),
}

/// Run `engine` for `query` (page 1) and write the cassette to
/// `<out_dir>/<engine_id>/<sha8(normalized_query)>.json`. Returns the path.
///
/// Engine-agnostic: works for any `Engine` impl, so `oxe record` only maps
/// an engine id to a constructor.
pub async fn record(
    engine: &dyn Engine,
    query: &str,
    out_dir: &Path,
) -> Result<PathBuf, RecordError> {
    let req = SearchRequest {
        q: query.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: Some(vec![engine.id()]),
        client: ClientKind::Cli,
    };
    let results = engine.search(&req, RECORD_BUDGET).await?;
    let path = cassette_path(out_dir, engine.id().as_str(), query);
    Cassette::new(engine.id(), query, results).save(&path)?;
    Ok(path)
}
