-- name: get-answer(key)^
SELECT response, expires_at FROM answers WHERE answer_hash = :key;

-- name: put-answer(key, text, response, model, created_at, expires_at)!
INSERT INTO answers (answer_hash, query_text, response, model, created_at, expires_at, hits)
VALUES (:key, :text, :response, :model, :created_at, :expires_at, 0)
ON CONFLICT(answer_hash) DO UPDATE SET
  response = excluded.response,
  model = excluded.model,
  created_at = excluded.created_at,
  expires_at = excluded.expires_at,
  hits = 0;

-- name: answer-hits-bump(key)!
UPDATE answers SET hits = hits + 1 WHERE answer_hash = :key;

-- name: count-answers()
SELECT COUNT(*) FROM answers;

-- name: prune-answers(now)!
DELETE FROM answers WHERE expires_at < :now;
