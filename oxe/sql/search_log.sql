-- name: log-search(ts, query_text, query_hash, source, backend, result_count, duration_ms, client)!
INSERT INTO search_log (ts, query_text, query_hash, source, backend, result_count, duration_ms, client)
VALUES (:ts, :query_text, :query_hash, :source, :backend, :result_count, :duration_ms, :client);

-- name: get-search-log(limit)
SELECT id, ts, query_text, query_hash, source, backend, result_count, duration_ms, client
FROM search_log ORDER BY ts DESC LIMIT :limit;

-- name: lookup-query-text-log(key)^
SELECT query_text FROM search_log WHERE query_hash = :key ORDER BY ts DESC LIMIT 1;

-- name: prune-search-log(cutoff)!
DELETE FROM search_log WHERE ts <= :cutoff;

-- name: has-search-log()
SELECT 1 FROM sqlite_master WHERE type='table' AND name='search_log';

-- name: stat-daily(cutoff)
SELECT ts, source, duration_ms FROM search_log WHERE ts >= :cutoff ORDER BY ts;

-- name: stat-top-queries(cutoff, limit)
SELECT query_text, COUNT(*) AS c FROM search_log WHERE ts >= :cutoff
GROUP BY query_hash ORDER BY c DESC, query_text LIMIT :limit;

-- name: stat-zero-result(cutoff, limit)
SELECT query_text, MAX(ts) FROM search_log WHERE ts >= :cutoff AND result_count = 0
GROUP BY query_hash ORDER BY MAX(ts) DESC LIMIT :limit;

-- name: suggest-queries(prefix, limit)
SELECT query_text, MAX(ts) AS last_ts, COUNT(*) AS freq
FROM search_log
WHERE query_text LIKE :prefix ESCAPE '\'
GROUP BY query_hash
ORDER BY last_ts DESC, freq DESC, query_text
LIMIT :limit;

-- name: stat-client-split(cutoff)
SELECT client, COUNT(*) FROM search_log WHERE ts >= :cutoff GROUP BY client;
