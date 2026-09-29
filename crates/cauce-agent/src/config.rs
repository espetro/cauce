//! [`LoopConfig`]: the [`AnswerLoop`](crate::AnswerLoop)'s tunables,
//! bundled so a swapped-in harness sees the same knobs.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

/// `answers` row TTL (settled input: 24 h).
pub const DEFAULT_ANSWERS_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Per-call provider budget, matching the engine `DEFAULT_DEADLINE` scale
/// and v2's `PROVIDER_TIMEOUT_S` (60 s).
pub const DEFAULT_PROVIDER_BUDGET: Duration = Duration::from_secs(60);
/// Tool-loop cap (settled input; v2 `MAX_ITERATIONS`).
pub const DEFAULT_MAX_ITERATIONS: usize = 5;

/// Every knob the loop reads, in one struct — the config half of the
/// harness seam. Defaults are the settled production values.
#[derive(Clone)]
pub struct LoopConfig {
    /// Per-call provider timeout (connect + stream), applied to every
    /// `chat_stream` invocation.
    pub provider_budget: Duration,
    /// `answers` row TTL applied when a fresh answer lands.
    pub answers_ttl: Duration,
    /// Max provider turns per run before the loop stops with a
    /// terminal error frame.
    pub max_iterations: usize,
    /// Final-turn tail-parse: `(answer, confidence,
    /// related_questions)` from the streamed content.
    /// [`crate::answer::parse_final_answer`]-compatible shape; tests
    /// and evals swap it to prove the metadata-tail gate.
    pub tail_parser: fn(&str) -> (String, u8, Vec<String>),
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            provider_budget: DEFAULT_PROVIDER_BUDGET,
            answers_ttl: DEFAULT_ANSWERS_TTL,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            tail_parser: crate::answer::parse_final_answer,
        }
    }
}
