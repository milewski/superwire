FROM node:25-alpine3.23 AS playground-builder

WORKDIR /workspace/playground

COPY playground/package.json playground/package-lock.json ./
RUN npm ci

COPY playground/ ./
COPY documentation/docs/public /workspace/documentation/docs/public
COPY editors/textmate/syntaxes/wire.tmLanguage.json /workspace/editors/textmate/syntaxes/wire.tmLanguage.json
RUN npm run build
RUN test -f dist/assets/logo-horizontal-*.svg

FROM rust:1.94-alpine3.23 AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./

COPY crates/superwire-core/Cargo.toml crates/superwire-core/Cargo.toml
COPY crates/superwire-lsp/Cargo.toml crates/superwire-lsp/Cargo.toml
COPY crates/superwire-cli/Cargo.toml crates/superwire-cli/Cargo.toml
COPY crates/superwire-executor/Cargo.toml crates/superwire-executor/Cargo.toml
COPY vendor/ vendor/

RUN mkdir -p crates/superwire-core/src && echo "" > crates/superwire-core/src/lib.rs \
    && mkdir -p crates/superwire-lsp/src && echo "" > crates/superwire-lsp/src/lib.rs \
    && echo "" > crates/superwire-lsp/src/main.rs \
    && mkdir -p crates/superwire-lsp/benches && echo "" > crates/superwire-lsp/benches/completion_filtering.rs \
    && mkdir -p crates/superwire-cli/src && echo "" > crates/superwire-cli/src/main.rs \
    && mkdir -p crates/superwire-executor/src && echo "" > crates/superwire-executor/src/lib.rs \
    && mkdir -p crates/superwire-executor/benches && echo "" > crates/superwire-executor/benches/runtime.rs \
    && cargo fetch \
    && rm -rf crates/*/src crates/*/benches

COPY crates/ crates/

RUN cargo build --release -p superwire-executor --locked \
    && cargo build --release -p superwire-cli --locked

RUN strip -s target/release/superwire-executor \
    && strip -s target/release/superwire-cli \
    && ls -lh target/release/superwire-executor target/release/superwire-cli

FROM alpine:3.23

RUN apk add --no-cache ca-certificates \
    && addgroup --gid 1000 superwire \
    && adduser --disabled-password --ingroup superwire --uid 1000 superwire

COPY --from=builder /workspace/target/release/superwire-executor /usr/local/bin/superwire-executor
COPY --from=builder /workspace/target/release/superwire-cli /usr/local/bin/superwire-cli
COPY --from=playground-builder /workspace/playground/dist /usr/local/share/superwire/playground

ENV SUPERWIRE_PLAYGROUND_DIST=/usr/local/share/superwire/playground

USER superwire

EXPOSE 13703

ENTRYPOINT ["/usr/local/bin/superwire-executor"]
