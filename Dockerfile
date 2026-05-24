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

COPY crates/superwire-types/Cargo.toml crates/superwire-types/Cargo.toml
COPY crates/superwire-dsl/Cargo.toml crates/superwire-dsl/Cargo.toml
COPY crates/superwire-test-support/Cargo.toml crates/superwire-test-support/Cargo.toml
COPY crates/superwire-macros/Cargo.toml crates/superwire-macros/Cargo.toml
COPY crates/superwire-semantic/Cargo.toml crates/superwire-semantic/Cargo.toml
COPY crates/superwire-mcp/Cargo.toml crates/superwire-mcp/Cargo.toml
COPY crates/superwire-protocol/Cargo.toml crates/superwire-protocol/Cargo.toml
COPY crates/superwire-model/Cargo.toml crates/superwire-model/Cargo.toml
COPY crates/superwire-provider-cersei/Cargo.toml crates/superwire-provider-cersei/Cargo.toml
COPY crates/superwire-executor-server/Cargo.toml crates/superwire-executor-server/Cargo.toml
COPY crates/superwire-lsp/Cargo.toml crates/superwire-lsp/Cargo.toml
COPY crates/superwire-cli/Cargo.toml crates/superwire-cli/Cargo.toml
COPY crates/superwire-executor/Cargo.toml crates/superwire-executor/Cargo.toml
COPY vendor/ vendor/

RUN mkdir -p crates/superwire-types/src && echo "" > crates/superwire-types/src/lib.rs \
    && mkdir -p crates/superwire-dsl/src && echo "" > crates/superwire-dsl/src/lib.rs \
    && mkdir -p crates/superwire-test-support/src && echo "" > crates/superwire-test-support/src/lib.rs \
    && mkdir -p crates/superwire-macros/src && echo "" > crates/superwire-macros/src/lib.rs \
    && mkdir -p crates/superwire-semantic/src && echo "" > crates/superwire-semantic/src/lib.rs \
    && mkdir -p crates/superwire-mcp/src && echo "" > crates/superwire-mcp/src/lib.rs \
    && mkdir -p crates/superwire-protocol/src && echo "" > crates/superwire-protocol/src/lib.rs \
    && mkdir -p crates/superwire-model/src && echo "" > crates/superwire-model/src/lib.rs \
    && mkdir -p crates/superwire-provider-cersei/src && echo "" > crates/superwire-provider-cersei/src/lib.rs \
    && mkdir -p crates/superwire-executor-server/src && echo "" > crates/superwire-executor-server/src/lib.rs \
    && echo "fn main() {}" > crates/superwire-executor-server/src/main.rs \
    && mkdir -p crates/superwire-lsp/src && echo "" > crates/superwire-lsp/src/lib.rs \
    && echo "fn main() {}" > crates/superwire-lsp/src/main.rs \
    && mkdir -p crates/superwire-lsp/benches && echo "" > crates/superwire-lsp/benches/completion_filtering.rs \
    && mkdir -p crates/superwire-cli/src && echo "" > crates/superwire-cli/src/lib.rs \
    && echo "fn main() {}" > crates/superwire-cli/src/main.rs \
    && mkdir -p crates/superwire-executor/src && echo "" > crates/superwire-executor/src/lib.rs \
    && mkdir -p crates/superwire-executor/benches && echo "" > crates/superwire-executor/benches/runtime.rs \
    && cargo fetch \
    && rm -rf crates/*/src crates/*/benches

COPY crates/ crates/

RUN cargo build --release -p superwire-executor-server --locked \
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
