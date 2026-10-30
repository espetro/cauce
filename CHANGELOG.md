# Changelog

## 0.9.0

Gemini-class AI UX (wave 8, UX-01 to UX-09).

- Server-authoritative conversation threads: `answer_thread` table, `GET /api/threads`, `GET` and `DELETE /api/threads/{id}`, and an optional `thread_id` on `POST /api/answer` (returned in `done.thread_id`). Threads are owner-scoped; a public instance never serves another user's thread.
- Follow-up turns now survive a reload and back/forward: the answer URL is `/app/answer?thread=<id>`, inside the SPA mount, with a localStorage fallback. A warning shows at 15 turns and the server's 20-turn cap is reported as a specific error.
- One conversation shell for search and AI mode: a prompt card on top, one composer that sits at the top on an empty thread and sticks to the bottom once turns exist.
- Sources moved out of the answer body into a right-side panel (bottom drawer below 700px), scoped to the selected turn, opened from a `Sources (N)` chip.
- Minimal search results: a single column of hairline rows with one meta line (`12 results · 140 ms · cached`). The cache-details expander, the stream-status line and the assist card are gone from the results page.
- Minimal nav: logo, Search, Answer, History on the left; theme toggle and a waffle menu (Dashboard, Archive, Engines, Cache, Audit, Settings, Docs) on the right, a full-screen sheet below 700px.
- `Conversations` section at the top of `/app/history`: resume or delete saved threads.
- Migration `0006_answer_threads.sql` appends the new table; existing databases upgrade in place.
