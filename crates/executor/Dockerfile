# syntax=docker/dockerfile:1.7

FROM rust:1.94-alpine3.23 AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /workspace

RUN mkdir -p .cargo

RUN echo '[source.crates-io]' >> .cargo/config.toml && \
    echo 'replace-with = "vendored-sources"' >> .cargo/config.toml && \
    echo '[source.vendored-sources]' >> .cargo/config.toml && \
    echo 'local-registry = "/usr/local/cargo/registry"' >> .cargo/config.toml

COPY Cargo.toml Cargo.lock ./

COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/lsp/Cargo.toml crates/lsp/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/executor/Cargo.toml crates/executor/Cargo.toml

RUN cargo fetch --locked

COPY crates/executor/src crates/executor/src

RUN cargo build --release -p superwire-executor --locked

RUN strip -s target/release/superwire-executor \
    && ls -lh target/release/superwire-executor

FROM alpine:3.23

RUN apk add --no-cache ca-certificates \
    && addgroup -S superwire \
    && adduser -S -G superwire superwire

COPY --from=builder /workspace/target/release/superwire-executor /usr/local/bin/superwire-executor

USER superwire

EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/superwire-executor"]

CMD ["--address", "0.0.0.0:3000"]