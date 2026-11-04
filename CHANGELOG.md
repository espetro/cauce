# Changelog

## 0.9.0

Gemini-class AI UX (wave 8, UX-01 to UX-09), public-instance readiness, and the design-system rebuild.

- Server-authoritative conversation threads: `answer_thread` table, `GET /api/threads`, `GET` and `DELETE /api/threads/{id}`, and an optional `thread_id` on `POST /api/answer` (returned in `done.thread_id`). Threads are owner-scoped; a public instance never serves another user's thread.
- Follow-up turns now survive a reload and back/forward: the answer URL is `/app/answer?thread=<id>`, inside the SPA mount, with a localStorage fallback. A warning shows at 15 turns and the server's 20-turn cap is reported as a specific error.
- One conversation shell for search and AI mode: a prompt card on top, one composer that sits at the top on an empty thread and sticks to the bottom once turns exist.
- Sources moved out of the answer body into a right-side panel (bottom drawer below 700px), scoped to the selected turn, opened from a `Sources (N)` chip.
- Minimal search results: a single column of hairline rows with one meta line (`12 results · 140 ms · cached`). The cache-details expander, the stream-status line and the assist card are gone from the results page.
- Minimal nav: logo, Search, Answer, History on the left; theme toggle and a waffle menu (Dashboard, Archive, Engines, Cache, Audit, Settings, Docs) on the right, a full-screen sheet below 700px.
- `Conversations` section at the top of `/app/history`: resume or delete saved threads.
- Migration `0006_answer_threads.sql` appends the new table; existing databases upgrade in place.

Public instance + deployment (PUB-01/PUB-02):

- `server.allowed_hosts` — named-host deployments behind a proxy (cloudflared, reverse proxies) no longer get `403 not a cauce listen name`; list the hosts HostGuard accepts for `Host`/`Origin` (restart-required).
- Public-instance readiness: per-IP token bucket (`[rate_limit]`), global in-flight cap (`server.max_inflight`), admission queueing with stale-serve fallback, per-route `Cache-Control` classes (`[edge]`).
- BYOK AI: `ai.allow_user_keys` + `ai.allow_user_base_url` gates honour per-request provider credentials (never stored server-side), with a per-IP `free_daily_answers` budget for instance-paid answers.
- Deploy artifacts: Dockerfile/compose/systemd/Caddy/Lambda under `deploy/` + `docs/deploy/` (vps, docker, cf-pages, cf-zone).

SPA rebuild (FX-05/FX-06, DS, UX waves, DS-20):

- The HTMX stack is gone; the Svelte 5 SPA at `/app` is the only UI (Tailwind v4 + vendored bits-ui `ui/` wrappers + sv-prompt-kit `ai/` composer foundation).
- DS-20 craft pass: a real type scale (base h1–h4 rules, `--text-*`/`--leading-*`/`--tracking-*`/`--control-h`/`--shadow-sm` tokens), landing hero copy naming the product, single-query search page (the prompt-card double-render is gone), two-line result rows, archive row grid, dashboard panel styling.
- `cauce tui` — terminal search + experimental AI mode (#288).
