-- name: get-cache(key)^
SELECT response, expires_at FROM cache WHERE query_hash = :key;

-- name: hits-bump(key)!
UPDATE cache SET hits = hits + 1 WHERE query_hash = :key;

-- name: put-cache(key, text, response, expires_at)!
INSERT INTO cache (query_hash, query_text, response, expires_at, hits)
VALUES (:key, :text, :response, :expires_at, 0)
ON CONFLICT(query_hash) DO UPDATE SET
  response = excluded.response,
  expires_at = excluded.expires_at,
  hits = 0;

-- name: delete-cache(key)!
DELETE FROM cache WHERE query_hash = :key;

-- name: count-cache()
SELECT COUNT(*) FROM cache;

-- name: list-rows(now, q, fuzzy_ids, limit, offset)
SELECT query_hash AS hash, query_text AS query, expires_at, hits,
       length(response) AS size_bytes
FROM cache
WHERE (CAST(:now AS INTEGER) = 0 OR expires_at >= :now)
  AND (:q IS NULL OR query_text LIKE :q OR query_hash IN (SELECT value FROM json_each(:fuzzy_ids)))
ORDER BY expires_at DESC
LIMIT :limit OFFSET :offset;

-- name: cache-stats(now)^
SELECT COUNT(*) AS rows,
       COALESCE(SUM(CASE WHEN expires_at >= :now THEN 1 ELSE 0 END), 0) AS unexpired_rows,
       COALESCE(SUM(hits), 0) AS total_hits,
       MIN(CASE WHEN expires_at >= :now THEN expires_at END) AS oldest_unexpired,
       MAX(expires_at) AS newest
FROM cache;

-- name: lookup-query-text-cache(key)^
SELECT query_text FROM cache WHERE query_hash = :key;

-- name: cache-count-for-hash(key)
SELECT COUNT(*) FROM cache WHERE query_hash = :key;

-- name: count-answers()
SELECT COUNT(*) AS c FROM answers;
