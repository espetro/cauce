//! `gen_i18n` — write the pages' `var S = {...}`/`var AS = {...}` string
//! bundles to `web/src/i18n/{search,assist,answer}.json` from the
//! rust-i18n catalog (`locales/en.yaml`). The TypeScript modules then
//! typecheck against literal copy and vitest asserts on real strings.
//!
//! Run by `mise run web` before the freshness `git diff --exit-code`;
//! regenerating by hand is `cargo run -p cauce-server --bin gen_i18n`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::Path;

fn main() {
    rust_i18n::set_locale("en");
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/src/i18n");
    std::fs::create_dir_all(&out_dir).unwrap();
    for (name, bundle) in [
        ("search", cauce_server::i18n::search_bundle()),
        ("assist", cauce_server::i18n::assist_bundle()),
        ("answer", cauce_server::i18n::answer_bundle()),
    ] {
        let mut json = serde_json::to_string_pretty(&bundle).unwrap();
        json.push('\n');
        std::fs::write(out_dir.join(format!("{name}.json")), json).unwrap();
    }
}
