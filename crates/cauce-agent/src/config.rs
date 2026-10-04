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
/// Provider turns the tool loop may take before it must synthesize
/// (#231; replaces the single `max_iterations` cap — a turn that only
/// emits text is cheap, so the loop gets headroom the old 5-turn cap
/// starved on complex queries).
pub const DEFAULT_MAX_TURNS: usize = 8;
/// Tool calls the loop may execute per run (#231): the real cost
/// budget — a `search_web`/`search_archive` dispatch hits the pipeline,
/// while a deduped or over-budget call resolves in-band for free.
pub const DEFAULT_MAX_SEARCH_EXECUTIONS: usize = 6;

/// Every knob the loop reads, in one struct — the config half of the
/// harness seam. Defaults are the settled production values.
#[derive(Clone)]
pub struct LoopConfig {
    /// Per-call provider timeout (connect + stream), applied to every
    /// `chat_stream` invocation.
    pub provider_budget: Duration,
    /// `answers` row TTL applied when a fresh answer lands.
    pub answers_ttl: Duration,
    /// Max provider turns the tool loop takes; on exhaustion one
    /// forced-synthesize turn (`tool_choice: "none"`) still runs, so
    /// this bounds the run at `max_turns + 1` provider calls.
    pub max_turns: usize,
    /// Max tool calls actually executed per run — a repeat query or an
    /// over-budget call gets an in-band error result instead of a
    /// pipeline hit.
    pub max_search_executions: usize,
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
            max_turns: DEFAULT_MAX_TURNS,
            max_search_executions: DEFAULT_MAX_SEARCH_EXECUTIONS,
            tail_parser: crate::answer::parse_final_answer,
        }
    }
}
