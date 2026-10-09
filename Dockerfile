# syntax=docker/dockerfile:1
# cauce — single-binary metasearch (UI + API + MCP).
#
#   docker build -t cauce .
#   docker run -p 127.0.0.1:4479:4479 -v cauce-data:/var/lib/cauce cauce
#
# Multi-arch: `docker buildx build --platform linux/amd64,linux/arm64`
# compiles under emulation — no cross-toolchain setup needed. The image
# runs public-instance mode by default (CAUCE_SERVER_PUBLIC_INSTANCE);
# override config via env or a mounted /etc/cauce/config.toml.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Engine YAML specs and assets are compile-time embedded (rust-embed).
COPY engines ./engines
RUN cargo build --release -p cauce-cli --locked

FROM debian:bookworm-slim AS runtime
# python3 + ddgs: the bundled `exec` engine
# (sdk/python/cauce_engine_sdk/ddgs_auto.py) needs both; the SDK itself is
# dependency-free. Drop `ddgs` or the whole python3 layer for a leaner
# image and disable the engine instead (`CAUCE_ENGINES=bing,brave`).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      python3 python3-pip ca-certificates curl \
 && pip3 install --break-system-packages --no-cache-dir "ddgs>=9" \
 && apt-get purge -y python3-pip && apt-get autoremove -y \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --home-dir /var/lib/cauce cauce \
 && install -d -o cauce -g cauce /var/lib/cauce /etc/cauce

WORKDIR /opt/cauce
# sdk/python is resolved relative to the process cwd by the bundled
# engine args, so keep this layout.
COPY sdk/python ./sdk/python
COPY --from=build /src/target/release/cauce /usr/local/bin/cauce

ENV CAUCE_CONFIG_DIR=/etc/cauce \
    CAUCE_DATA_DIR=/var/lib/cauce \
    CAUCE_LOG=info \
    CAUCE_SERVER_PUBLIC_INSTANCE=true
EXPOSE 4479
VOLUME ["/var/lib/cauce"]
USER cauce
HEALTHCHECK --interval=30s --timeout=3s \
  CMD curl -fsS http://127.0.0.1:4479/health || exit 1
ENTRYPOINT ["cauce", "serve", "--bind", "0.0.0.0"]
