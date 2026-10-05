# 2026-10-04 — deterministic groundedness gate (#232, branch v3/groundedness)

- `cauce_agent::groundedness(answer, sources)` is the `answers` write gate now:
  ≥1 source AND every prose sentence carries an in-range `[n]` AND every `[n]`
  resolves (`1 <= n <= sources.len()`). Strict coverage (== 1.0) is deliberate —
  the #224 failure mode was "one cite + confidence ≥4 caches for 24h", which
  `coverage > 0` would not fix. Verbalized confidence is display-only.
- The strictness cost is real: gpt-4o-mini habitually leaves a framing or
  closing sentence uncited ("For a detailed forecast, check local services."),
  so ~2/4-cited answers now refuse to cache. Flagged in PR #249; if 24h hit-rate
  suffers, a coverage threshold knob belongs in config, not in this fn.
- Sentence grammar lives in `grounded.rs`: `.!?` + whitespace/EOL splits,
  suppressed after a small ABBREVIATIONS list or a single-letter pre-dot token
  (initials + the tail of `e.g.`/`U.S.` — the token before the outer dot is one
  letter, so no dotted-abbrev list needed). Fenced blocks + inline code are
  blanked before splitting, mirroring `render_answer_html`'s no-cite-in-code
  rule; `[n]` is the ONLY cite form (`[1, 2]` is not a cite).
- `parse_final_answer` tail handling: `split_trailing_fence` (parity-tracked)
  peels a closed ```json fence around the metadata object;
  `strip_dangling_fence_opener` drops an unclosed opener left before a bare
  tail; `drop_unrecoverable_metadata_block` drops an unclosed fence whose
  contents `looks_like_metadata` (`{` + names `confidence`, quote-tolerant so a
  broken tail still drops rather than leaking raw JSON into `done.answer`).
- `parse_confidence` accepts floats + numeric strings (`f.round() as i64`,
  non-finite rejected); the old `as_i64()` path dropped a `7.5` tail and leaked
  it into the answer.
- Devin's git layer rewrites the committer identity to the user's GitHub
  noreply (`26749475+espetro@users.noreply.github.com`) regardless of
  `-c user.email` — the `Signed-off-by` trailer follows the committer, not the
  `--author` line. Author stays controllable via `--author`.
- `cauce-server::report api_report_days_bounds_the_window` is a wall-clock
  test: fixture hardcodes `cauce-2026-09-28.jsonl` inside `?days=1` — fails on
  main since ~2026-09-30 (seen failing on main's own validate run 2026-10-04).
  Needs a today-relative fixture, not a static filename.
- e2e tactic that worked: seed replay cassettes for the phrasings the MODEL
  ACTUALLY searches (watch `step` frame `query` fields), not the ones you guess.
  gpt-4o-mini declines to cite replay's synthetic fallback results (smart — it
  refused to ground nonsense), so first runs land ungrounded. The Assist turn
  (`context_results` body) is the deterministic grounded path: it cites the
  numbered context and one call proves write + `cached:true` replay.
