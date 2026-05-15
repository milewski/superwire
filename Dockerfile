FROM node:25-alpine3.23 AS playground-builder

WORKDIR /workspace/playground

COPY playground/package.json playground/package-lock.json ./
RUN npm ci

COPY playground/ ./
COPY editors/textmate/syntaxes/wire.tmLanguage.json /workspace/editors/textmate/syntaxes/wire.tmLanguage.json
RUN npm run build

FROM rust:1.94-alpine3.23 AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./

COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/lsp/Cargo.toml crates/lsp/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/executor/Cargo.toml crates/executor/Cargo.toml

RUN mkdir -p crates/core/src && echo "" > crates/core/src/lib.rs \
    && mkdir -p crates/lsp/src && echo "" > crates/lsp/src/lib.rs \
    && mkdir -p crates/cli/src && echo "" > crates/cli/src/main.rs \
    && mkdir -p crates/executor/src && echo "" > crates/executor/src/lib.rs \
    && cargo fetch \
    && rm -rf crates/*/src

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

EXPOSE 3000
EXPOSE 3001

ENTRYPOINT ["/usr/local/bin/superwire-executor"]

CMD ["--address", "0.0.0.0:3000"]
