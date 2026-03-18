FROM lukemathwalker/cargo-chef:latest-rust-alpine3.21 AS chef
WORKDIR /app
RUN apk add --no-cache \
    build-base \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    ca-certificates \
    curl \
    pkgconfig

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release
RUN cp target/release/veloz /app/veloz

FROM alpine:latest AS runtime
WORKDIR /app
COPY --from=builder /app/veloz /app/veloz
RUN apk add --no-cache ca-certificates openssl

EXPOSE 81

ENTRYPOINT ["/app/veloz"]
