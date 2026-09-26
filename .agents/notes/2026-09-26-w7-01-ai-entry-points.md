# W7-01 first-class AI mode entry points (2026-09-26)

Findings from implementing issue #199 (`v3/w7-01-ai-entry`).

## `header.html` is a shared include — new fields are an N-struct contract

`templates/header.html` is `{% include %}`d by 10 page templates (page,
answer, history, settings, dashboard, engines, cache, audit, trace —
both trace inits — and W5-02's archive). Any field the header
references must exist on **every** including struct; adding
`answer_available` touched `Page`, `AnswerPage`, `Dashboard`
(`from_snapshot` gained the param), `EnginesPage`, `AuditPage`,
`TracePage` (incl. `trace_frame`), `CachePage`, `History`,
`SettingsPage` and `Archive`. The non-`archive`-feature `archive()`
handler had no `State` extractor — it needed one to compute
`state.answer().is_some()`.

## `ai_enabled` vs `answer_available` are different semantics

`SettingsPage.ai_enabled` already existed and means the `ai.enabled`
*config flag* (the settings checkbox). The header gate must reflect a
live answer loop (`state.answer().is_some()` — same condition as the
`ask_url()` link), so it is named `answer_available`. Don't merge the
two: config-on-but-loop-failed must still hide the entry points.

## Gate the JS, not just the markup

`page.html`'s pill wiring and the `data.mode === "ai"` submit branch
are inside `{% if answer_available %}` blocks — disabled builds ship
neither `id="ai-mode"` nor the `/answer?q=` literal. That keeps
`search_page_ask_link_follows_ai`'s `!body.contains("/answer?q=")`
assertion true without touching the test.

## Live smoke without a provider

`CAUCE_AI_ENABLED=true` + `CAUCE_AI_BASE_URL=http://127.0.0.1:9`
(unreachable stub) + any key/model: the provider client builds, the
loop exists, `answer()` is `Some`, and every entry point renders — the
stream never needs to succeed for markup checks. `CAUCE_AI_ENABLED=false`
gives the disabled surface (no nav link, no pill, `/answer` shows the
disabled notice).

## Base worktrees on `origin/main`, not local `main`

`git worktree add ... main` used a stale local `main` whose history had
since been rewritten upstream — the first push produced a PR diff of
377 files (114 phantom commits). Fix was
`git rebase --onto origin/main <old-base>` + `--force-with-lease`.
Lesson: `git fetch` and branch the worktree off `origin/main` directly.
