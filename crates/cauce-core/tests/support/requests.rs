//! Request and engine-construction helpers for the `SearchPipeline`
//! integration tests.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use cauce_core::{ClientKind, SafeSearch, SearchRequest};
use cauce_engines::{Replay, ReplayOpts};

pub fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Api,
    }
}

/// A `Replay` engine rooted at `root` (no cassettes found → synthetic
/// mode, or cassette dirs created by the test).
pub fn replay_at(root: &std::path::Path, f: impl FnOnce(&mut ReplayOpts)) -> Replay {
    let mut opts = ReplayOpts {
        fixtures_root: root.to_path_buf(),
        ..ReplayOpts::default()
    };
    f(&mut opts);
    Replay::new(opts)
}
