# Cross-compiling using Docker multi-platform builds/images and `xx`.
#
# https://docs.docker.com/build/building/multi-platform/
# https://github.com/tonistiigi/xx
FROM --platform=${BUILDPLATFORM:-linux/amd64} tonistiigi/xx AS xx

# Utilizing Docker layer caching with `cargo-chef`.
#
# https://www.lpalmieri.com/posts/fast-rust-docker-builds/
FROM --platform=${BUILDPLATFORM:-linux/amd64} lukemathwalker/cargo-chef:latest-rust-slim-bookworm AS chef


FROM chef AS planner
WORKDIR /src
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
WORKDIR /src

COPY --from=xx / /
RUN apt-get update && apt-get install -y clang lld cmake

# `ARG`/`ENV` pair is a workaround for `docker build` backward-compatibility.
#
# https://github.com/docker/buildx/issues/510
ARG BUILDPLATFORM
ENV BUILDPLATFORM=${BUILDPLATFORM:-linux/amd64}
RUN case "$BUILDPLATFORM" in \
        */amd64 ) PLATFORM=x86_64 ;; \
        */arm64 | */arm64/* ) PLATFORM=aarch64 ;; \
        * ) echo "Unexpected BUILDPLATFORM '$BUILDPLATFORM'" >&2; exit 1 ;; \
    esac;

# `ARG`/`ENV` pair is a workaround for `docker build` backward-compatibility.
#
# https://github.com/docker/buildx/issues/510
ARG TARGETPLATFORM
ENV TARGETPLATFORM=${TARGETPLATFORM:-linux/amd64}

RUN xx-apt-get install -y gcc g++ libc6-dev pkg-config libssl-dev

ENV OPENSSL_INCLUDE_DIR=/usr/include/openssl
ENV AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/aarch64-linux-gnu/openssl
ENV X86_64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/x86_64-linux-gnu/openssl
ENV AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR=/usr/lib/aarch64-linux-gnu
ENV X86_64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu
ENV OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu

COPY --from=planner /src/recipe.json recipe.json
RUN xx-cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN xx-cargo build --release --locked -p idempotent-proxy-server \
    && xx-verify target/$(xx-cargo --print-target-triple)/release/idempotent-proxy-server \
    && mv target/$(xx-cargo --print-target-triple)/release /src/release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata curl openssl \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENV AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR=/usr/lib/aarch64-linux-gnu
ENV OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu

WORKDIR /app
COPY --from=builder /src/.env ./.env
COPY --from=builder /src/release/idempotent-proxy-server ./idempotent-proxy-server

ENTRYPOINT ["./idempotent-proxy-server"]
