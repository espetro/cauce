//! Shared test support for the `SearchPipeline` integration tests,
//! split by role: [`engines`] holds `Engine` test doubles wrapping a
//! `Replay`, [`store`] the recording in-memory `Store` stub, and
//! [`requests`] the request/engine builders.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod engines;
mod requests;
mod store;

// Each test binary compiles this module separately; not every binary
// uses every export.
#[allow(unused_imports)]
pub use engines::{DialEngine, GateEngine};
#[allow(unused_imports)]
pub use requests::{replay_at, req};
#[allow(unused_imports)]
pub use store::StubStore;
