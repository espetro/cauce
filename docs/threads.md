# Conversation threads

From 0.9.0 a follow-up question continues a saved conversation instead of a one-off answer.

## How it works

The browser keeps the live transcript in memory and sends the full `history` with every `POST /api/answer`. Pass a `thread_id` and the server stores the completed turn in the `answer_thread` table and returns the id in the `done` frame. Without `thread_id` nothing is stored and behaviour is the same as before.

The web UI uses `/app/answer?thread=<id>`. Reloading or navigating back restores the conversation from the server. If the server read fails, the UI falls back to the copy of the transcript kept in the browser's local storage.

## API

- `GET /api/threads` lists your threads, newest first.
- `GET /api/threads/{id}` returns one thread with its turns.
- `DELETE /api/threads/{id}` deletes it.

Threads belong to their owner. On a local instance the owner is `local`. On a public instance it is derived from the bearer token (hashed, never stored raw), and another user's id answers `404`.

## Limits

A request may carry at most 20 turns and 64 KiB of history. The UI warns at 15 turns and reports the server's `400 bad_request` as a specific message.

## Where to find them

The `Conversations` section at the top of `/app/history` lists your threads; each row resumes the conversation and can be deleted.
