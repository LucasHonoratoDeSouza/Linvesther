# Builds the binance-worker binary for infra/docker-compose.yml's
# `binance-worker` service (the `scheduler` subcommand, running
# continuously). Built from the `services/` workspace root so path
# dependencies (`../live-client`, `../../credentials`, etc.) resolve.
FROM rust:1.90-slim AS build
WORKDIR /build
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release -p binance-worker

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
# sqlx::migrate!("./migrations") embeds the SQL files into the binary
# at compile time — nothing further to copy for migrations at runtime.
COPY --from=build /build/target/release/binance-worker /usr/local/bin/binance-worker
ENTRYPOINT ["binance-worker"]
CMD ["scheduler"]
