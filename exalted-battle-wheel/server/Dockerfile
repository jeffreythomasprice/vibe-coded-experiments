# Build context is the workspace root (see README.md) since the build needs `shared/` and the
# root Cargo.toml/Cargo.lock alongside `server/`.

FROM rust:1-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release -p server --recipe-path recipe.json
COPY . .
RUN cargo build --release -p server

FROM debian:bookworm-slim AS runtime
# Needed for the DynamoDB client's TLS: aws-smithy-http-client's default connector loads the system
# trust store (rustls-native-certs) rather than vendoring its own root certificates.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --uid 10001 --system --shell /usr/sbin/nologin app
COPY --from=builder /app/target/release/server /usr/local/bin/server
USER app
EXPOSE 8001
ENTRYPOINT ["/usr/local/bin/server"]
