//! Shared test support for the `cauce-server` integration tests.
//!
//! Module map (#157): [`state`] builds tempdir-backed `AppState`s
//! (SqliteStore + the deterministic `replay` engine); [`http`] is the
//! `oneshot` fetch layer (`call`, `get`, `get_html`, `get_json`,
//! `put_form`, `assert_envelope`); [`env`] serialises `CAUCE_*`
//! process-env mutation behind `env_lock` and writes sandbox
//! `config.toml`s. Each `tests/*.rs` binary compiles this module
//! separately and uses a subset — the same pattern as
//! `cauce-core/tests/support/`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod env;
mod http;
mod state;

#[allow(unused_imports)]
pub use env::*;
#[allow(unused_imports)]
pub use http::*;
#[allow(unused_imports)]
pub use state::*;
