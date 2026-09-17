-- name: record-click(hash, result_id, url, title, clicked_at, source)!
INSERT INTO clicks (query_hash, result_id, url, title, clicked_at, source)
VALUES (:hash, :result_id, :url, :title, :clicked_at, :source);

-- name: get-clicks(hash, q, since, limit)
SELECT c.id, c.query_hash, COALESCE(k.query_text, '') AS query,
       c.result_id, c.url, c.title, c.clicked_at, c.source
FROM clicks c
LEFT JOIN cache k ON k.query_hash = c.query_hash
WHERE (:hash IS NULL OR c.query_hash = :hash)
  AND (:q IS NULL OR k.query_text LIKE :q)
  AND (CAST(:since AS INTEGER) = 0 OR c.clicked_at >= :since)
ORDER BY c.clicked_at DESC
LIMIT :limit;

-- name: click-stats(cutoff)^
SELECT COUNT(*) AS total,
       SUM(CASE WHEN clicked_at >= :cutoff THEN 1 ELSE 0 END) AS last_24h,
       MIN(clicked_at) AS oldest
FROM clicks;

-- name: prune-clicks(cutoff)!
DELETE FROM clicks WHERE clicked_at <= :cutoff;

-- name: delete-clicks-all()!
DELETE FROM clicks;

-- name: delete-clicks-since(since)!
DELETE FROM clicks WHERE clicked_at >= :since;
