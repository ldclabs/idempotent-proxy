# syntax=docker/dockerfile:1

FROM --platform=$BUILDPLATFORM rust:slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y gcc g++ libc6-dev pkg-config libssl-dev

WORKDIR /src
COPY src ./src
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --locked
RUN ls target/release

FROM scratch AS exporter
WORKDIR /app
COPY --from=builder /src/target/release/idempotency-proxy ./
