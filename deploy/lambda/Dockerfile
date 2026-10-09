# syntax=docker/dockerfile:1
# cauce on AWS Lambda via the Lambda Web Adapter (LWA).
#
#   docker build -f deploy/lambda/Dockerfile -t cauce-lambda .
#   → push to ECR, create a `provided` (custom-runtime) function from the
#     image, enable a Function URL (no response streaming config needed —
#     AWS_LWA_INVOKE_MODE below streams SSE through it).
#
# The musl-free gnu build is fine: Lambda `provided` runtimes are glibc.
# Cold start ≈ binary init only; the TTL cache and any MCP sessions are
# per-warm-environment (docs/deploy/lambda.md has the caveats).

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY engines ./engines
RUN cargo build --release -p cauce-cli --locked

FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1 AS lwa

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      python3 python3-pip ca-certificates \
 && pip3 install --break-system-packages --no-cache-dir "ddgs>=9" \
 && apt-get purge -y python3-pip && apt-get autoremove -y \
 && rm -rf /var/lib/apt/lists/*

COPY --from=lwa /lambda-adapter /opt/extensions/lambda-adapter
WORKDIR /opt/cauce
COPY sdk/python ./sdk/python
COPY --from=build /src/target/release/cauce /usr/local/bin/cauce

ENV CAUCE_CONFIG_DIR=/tmp/cauce-cfg \
    CAUCE_DATA_DIR=/tmp/cauce-data \
    CAUCE_LOG=info \
    CAUCE_SERVER_PUBLIC_INSTANCE=true \
    CAUCE_SERVER_PORT=8080 \
    AWS_LWA_INVOKE_MODE=response_stream \
    AWS_LWA_READINESS_CHECK_PATH=/health \
    AWS_LWA_READINESS_CHECK_PORT=8080 \
    AWS_LWA_READINESS_CHECK_PROTOCOL=http

# /tmp is the only writable fs on Lambda — state does not survive a cold
# start, which is the intended shape for a TTL cache.
ENTRYPOINT ["cauce", "serve", "--bind", "0.0.0.0"]
