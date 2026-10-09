# AWS Lambda (Web Adapter)

The released binary runs on Lambda unchanged via the [Lambda Web
Adapter](https://github.com/awslabs/aws-lambda-web-adapter) — no
`provided` handler code, no porting. `deploy/lambda/Dockerfile` builds
the image.

```bash
docker build -f deploy/lambda/Dockerfile -t cauce-lambda .
docker tag cauce-lambda <acct>.dkr.ecr.<region>.amazonaws.com/cauce:latest
docker push <acct>.dkr.ecr.<region>.amazonaws.com/cauce:latest
```

Create the function from the image (custom runtime — the adapter is
baked in), attach a **Function URL** (auth NONE for a public instance,
or IAM for private), done. `AWS_LWA_INVOKE_MODE=response_stream` is set
in the image so SSE (`/api/search/stream`) streams through the URL
instead of buffering.

## Why it fits the req/mo budget

1M requests + 400k GB-seconds permanent free tier; a cauce request is a
sub-second 256–512 MB invocation → essentially free at hobbyist scale.
Putting CloudFront in front ([cf-zone.md](cf-zone.md), same recipe)
serves repeat queries without touching an invocation at all.

## Caveats (by design, not bugs)

- **Cache is ephemeral**: `cauce.db` lives on `/tmp` per warm
  environment — a cold start resets the TTL cache and any SQLite state.
  Correct for a TTL cache; do not put irreplaceable state there.
- **MCP sessions** are per-warm-env too (`LocalSessionManager`): a
  client pinned across cold starts re-initializes — normal MCP
  behaviour, but chatty.
- **`server.max_inflight` still matters**: set it (~50) so one warm env
  sheds instead of timing out the Lambda; horizontal burst scaling is
  Lambda's job, the cap protects warm-env health.
- **Engine egress timeouts**: keep `search.deadline_ms` comfortably
  under the function timeout (default LWA-ready 30 s; search deadline
  is 3 s).
- **No arm64 fuss needed** but it's free: build `--platform
  linux/arm64` and run the function on Graviton for ~20% cheaper GB-s.

## When not to use it

Sustained traffic past the free tier: a €3.5 VPS behind a CF zone is
flatter-priced and keeps SQLite state permanently. Lambda wins for
spiky/low-volume instances and zero-patch-surface deployments.
