Summary of all requested changes

Change 1 — Python: fix real leaks and try/finally into context managers

Files and edits:

╭──────────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ File:line                    │ Edit                                                                                                       │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/server/ai.py:137         │ Wrap AsyncOpenAI(...) in async with ... as client: (per-request /v1/models)                                │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/server/ai.py:146         │ Wrap AsyncAnthropic(...) in async with ... as client:                                                      │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/ai.py:516                │ Wrap sync OpenAI(...) in with ... as client: (per-call /settings/test)                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/stats.py:311             │ Replace try: conn = sqlite3.connect(...) ... finally: conn.close() with with sqlite3.connect(...) as conn: │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/stats.py:373             │ Same — convert try/finally to with sqlite3.connect(...) as conn:                                           │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ oxe/server/app.py (lifespan) │ Add state.cache.close() to the existing @asynccontextmanager lifespan's finally block                      │
╰──────────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

Not touched (already idiomatic or keep as-is):
• All 9 with self._lock sites in oxe/cache.py
• The two @asynccontextmanager lifespans in oxe/server/app.py:52, 64
• oxe/ai.py:323 try/finally on threading.Event (completion signal, not resource cleanup)

Change 2 — UI: bump tsconfig.app.json target/lib to esnext

2-line edit in ui/tsconfig.app.json:

-    "target": "es2023",
+    "target": "esnext",
-    "lib": ["ES2023", "DOM"],
+    "lib": ["ESNext", "DOM"],

Optional (no functional effect):
• Same shape edit in ui/tsconfig.node.json (only types vite.config.ts)
• vite.config.ts build.target: "esnext" (real bundle savings only if you write ES2024+ code)

What it does:
• Unlocks TS types for Promise.withResolvers, Object.groupBy, toSorted/toReversed/findLast, Array.fromAsync, #private fields, etc.
• Stops tsc from rejecting await using as a syntax error
• Does not change emitted JS — Vite's baseline-widely-available default already emits modern JS
• Does not affect bundle size or browser support

Change 3 — UI: fix stream reader leak with await using

Depends on Change 2 being applied first (needs target: "esnext" to typecheck).

In ui/src/lib/ai.ts:91-93, wrap res.body.getReader() so reader.cancel() runs on any error path (including JSON parse failure mid-stream). Small StreamReader class with [Symbol.dispose]() wrapping reader.cancel(), then await using reader = ....

─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

Suggested PR split

If you want atomic commits:

1. PR 1 — Python: AI client leaks + stats.py try/finally (Change 1, all of it; one logical change category)
2. PR 2 — UI: bump tsconfig target/lib (Change 2; prerequisite for Change 3)
3. PR 3 — UI: stream reader await using (Change 3)

Or, if you want tighter:

• PR A — Python context managers (Change 1)
• PR B — UI esnext + stream reader (Changes 2 + 3 together, since 3 depends on 2)

I lean toward the split version — Change 1 mixes a real bug fix (the AI client leaks) with a refactor (the try/finally → with), so they're separate logical commits even though they're in the same PR. Want me to draft any of these?
