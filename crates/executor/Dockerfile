# syntax=docker/dockerfile:1.7

FROM rust:1.94-alpine3.23 AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /workspace

RUN mkdir -p .cargo

COPY Cargo.toml Cargo.lock ./

COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/lsp/Cargo.toml crates/lsp/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/executor/Cargo.toml crates/executor/Cargo.toml

RUN mkdir -p crates/core/src && echo "" > crates/core/src/lib.rs \
    && mkdir -p crates/lsp/src && echo "" > crates/lsp/src/lib.rs \
    && mkdir -p crates/cli/src && echo "" > crates/cli/src/main.rs \
    && mkdir -p crates/executor/src && echo "" > crates/executor/src/lib.rs \
    && cargo build --release -p superwire-executor --locked 2>/dev/null || true \
    && cargo build --release -p superwire-cli --locked 2>/dev/null || true \
    && rm -rf crates/*/src

COPY crates/ crates/

RUN cargo build --release -p superwire-executor --locked \
    && cargo build --release -p superwire-cli --locked

RUN strip -s target/release/superwire-executor \
    && strip -s target/release/superwire-cli \
    && ls -lh target/release/superwire-executor target/release/superwire-cli

FROM alpine:3.23

RUN apk add --no-cache ca-certificates \
    && addgroup -S superwire \
    && adduser -S -G superwire superwire

COPY --from=builder /workspace/target/release/superwire-executor /usr/local/bin/superwire-executor
COPY --from=builder /workspace/target/release/superwire-cli /usr/local/bin/superwire-cli

USER superwire

EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/superwire-executor"]

CMD ["--address", "0.0.0.0:3000"]
