//! `export_bindings` — regenerates the TypeScript wire types under
//! `web/src/types/` from the `#[derive(TS)]` types (`cauce-core` response /
//! answer types, `cauce-server`'s `ApiError`). `mise run web` runs this test
//! and then `git diff --exit-code` to freshness-check the committed output;
//! run `cargo test -p cauce-server --test wire_types` after changing a wire
//! type. `.ts` files are the artifacts — nothing consumes them until the
//! step-3 rewrite (#214).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;

use ts_rs::TS;

const OUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/web/src/types");

/// MPL-2.0 banner matching `web/build.mjs`'s `postBanner` convention.
const BANNER: &str = "// This Source Code Form is subject to the terms of the Mozilla Public\n\
                      // License, v. 2.0. If a copy of the MPL was not distributed with this\n\
                      // file, You can obtain one at <https://mozilla.org/MPL/2.0/>.\n\n";

#[test]
fn export_bindings() {
    let cfg = ts_rs::Config::new().with_out_dir(OUT_DIR);
    // Top-level wire shapes; `export_all` emits every transitive
    // dependency (SearchResult, EngineId, EngineStatus, ...) as its own
    // file.
    for result in [
        cauce_core::SearchResponse::export_all(&cfg),
        cauce_core::StreamMeta::export_all(&cfg),
        cauce_core::ResultsFrame::export_all(&cfg),
        cauce_core::AnswerFrame::export_all(&cfg),
        // W7-04: the client-replayed thread turns on `AnswerBody` —
        // `answer.ts` imports this for the `history` payload.
        cauce_core::AnswerTurn::export_all(&cfg),
        cauce_server::ApiError::export_all(&cfg),
    ] {
        result.expect("binding export");
    }

    // ts-rs writes no license header; prepend the MPL banner the repo
    // requires on committed files.
    for entry in std::fs::read_dir(OUT_DIR).expect("web/src/types exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ts") {
            continue;
        }
        let body = std::fs::read_to_string(&path).unwrap();
        if !body.starts_with(BANNER) {
            std::fs::write(&path, format!("{BANNER}{body}")).unwrap();
        }
    }
    assert!(
        Path::new(OUT_DIR).join("SearchResult.ts").exists(),
        "expected web/src/types/SearchResult.ts to be written"
    );
}
