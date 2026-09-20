# Builds services/collector/binance/worker's Rust binary and apps/api's
# Node server into one runtime image. Two build stages run in
# parallel (Docker builds independent stages concurrently), then the
# final stage just copies the two outputs — keeps the shipped image
# small even though the Rust build stage itself is heavy.

FROM rust:1.90-slim-bookworm AS rust-build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /repo
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY services ./services
COPY zkvm ./zkvm
# Excludes the "proving" feature (RISC Zero): it needs its own toolchain
# (`rzup install rust`) that isn't in this image, and the zero-knowledge
# proof endpoint isn't needed for the rest of the app to work. See
# services/collector/binance/worker/Cargo.toml's own doc comment.
WORKDIR /repo/services
RUN cargo build --release -p binance-worker --no-default-features

FROM node:24-slim AS node-build
RUN corepack enable
WORKDIR /repo
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./
COPY apps/api/package.json apps/api/package.json
COPY services/chain-worker/package.json services/chain-worker/package.json
COPY tests/e2e/package.json tests/e2e/package.json
COPY packages packages
RUN pnpm install --frozen-lockfile
COPY apps/api apps/api
COPY services/chain-worker services/chain-worker

FROM node:24-slim AS runtime
RUN corepack enable
WORKDIR /app
COPY --from=node-build /repo /app
COPY --from=rust-build /repo/services/target/release/binance-worker /app/services/target/release/binance-worker
ENV BINANCE_WORKER_BINARY_PATH=/app/services/target/release/binance-worker
EXPOSE 4301
WORKDIR /app/apps/api
CMD ["npx", "tsx", "src/server.ts"]
