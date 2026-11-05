# Usage counts: what an instance can know about its users

Date: 2026-11-04 · Issue: #334 (W9-06) · Status: assessment, awaiting the owner's decision

## The question

Can cauce keep a count or estimate of how many users (humans plus agents) are using each
instance?

## What is knowable today, with no schema change

`StatsSnapshot.by_client` (`crates/cauce-core/src/store.rs:862`) groups `search_log` rows by
the coarse `client` label and counts them. The label is the inbound surface, not a person:
`ui`, `api`, `cli`, or `mcp:<name>` where `<name>` is the agent harness that performed the
MCP handshake (`mcp:claude-code`, `mcp:Bifrost-search-user`, ...). `search_log.origin`
(`user` vs `agent`) splits the same rows by pipeline entry point.

That yields two honest aggregates:

- **activity counts by surface**: how many searches came from the UI, the bare API, and
  each named MCP harness, over any window `GET /api/stats` accepts.
- **human versus agent split**: `origin = user` versus `origin = agent` (with the caveat
  that an agent driving the UI still counts as `user`).

Both are already exposed to admins; neither is a user count. Ten humans behind one browser
profile and one human running ten searches a day are indistinguishable: the log never
records a client IP, user id, session id, or bearer-token hash. `mcp:<name>` names the
*software* the agent used, not the agent's owner.

On a public instance (`server.public_instance = true`) there is nothing to count at all:
the pipeline runs with `.with_history_logging(!public_instance)` (`app.rs:356`), so
`search_log` and `answer_log` are never written, and `GET /api/stats` is admin-scoped
(401 anonymously). cauce.fyi therefore has zero rows to aggregate — by design, not by
oversight.

## What a real user count would need

1. **An identity column on the request path.** A hashed client id (cookie, bearer-token
   hash) or an IP prefix, written into `search_log` alongside `client`. This is a schema
   change plus a new write on the hot path.
2. **A privacy decision for public instances.** Any identity column turns cauce.fyi's
   dropped-by-design logs into pseudonymous behavioural records. The current posture —
   public mode writes no history — is the privacy story; the assessment should not quietly
   undo it.
3. **Counting outside the search path.** Searches are not users: a page-view counter (SPA
   route loads), MCP session handshakes (`initialize` per client id), and `answer_log`
   writes (local mode only) each say something the search log cannot — that someone
   *arrived*, even if they never searched.

## Recommendation

Do **not** add identity columns to the public instance. The aggregates already computed are
the right granularity for a privacy-first instance:

- Expose `by_client` and the `origin` split on the dashboard's instance card, labelled
  explicitly as **activity counts** (searches by surface), not user counts.
- Leave per-user counting to self-hosters, who own their logs and can add an identity
  column in a fork or a follow-up without changing the public posture.
- If the owner wants a lightweight public-instance signal later, the honest candidates are
  MCP `initialize` handshake counts (session-level, no query content) and a page-view
  counter; both should ship behind a config flag defaulting to off.

## Verified facts this assessment rests on

- `search_log` columns: no IP, user, session, or token-hash field (`store.rs:61-117`).
- `by_client`/`ClientCount` aggregation exists (`store.rs:695-699, 862`).
- Public mode disables history logging at pipeline construction (`app.rs:356`).
- `GET /api/stats` is admin-gated (`routes.rs`), so the numbers never leak anonymously.
