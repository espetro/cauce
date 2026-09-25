[Docs](https://example.com/docs) / Rate limiting

Otter's API applies a token bucket per API key: 120 requests per minute sustained, with a burst allowance of 30. When you exceed the bucket the API returns `429 Too Many Requests` and a `Retry-After` header with the number of seconds to wait.

Every response carries the bucket state so a client can preemptively slow down instead of hitting the limit:

```
X-RateLimit-Limit: 120
X-RateLimit-Remaining: 87
X-RateLimit-Reset: 1729038060
Retry-After: 14
```

`X-RateLimit-Reset` is a unix timestamp — the moment the window fully refills. `Retry-After` is present only on 429 responses.

## Retry strategy

On a 429, wait `Retry-After` seconds, then retry once. If the second attempt also fails, apply exponential backoff starting at 2 seconds and doubling to a ceiling of 60 seconds. Honour `Retry-After` exactly: the clock resets each time a request lands early, so hammering the endpoint keeps you blocked indefinitely.

## Scope of the bucket

Limits apply per API key, not per IP or per endpoint. All endpoints share one bucket, including `GET` requests — with two exceptions: `/health` and `/openapi.json` are unmetered and safe for load-balancer health checks.

> **Batching tip:** if you find yourself brushing the limit, switch to `POST /v1/batch`, which accepts up to 100 operations per call and counts once against the bucket.

Accounts on the Scale plan can request a raised bucket (240/min sustained, burst 60) from the dashboard under Settings → Limits. Approval is usually same-day.

[← Pagination](https://example.com/docs/pagination) [Webhooks →](https://example.com/docs/webhooks)