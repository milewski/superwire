FROM rust:1.94-bookworm

ARG PHP_VERSION=8.4.19

RUN apt update && apt install -y --no-install-recommends \
    clang build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN cd /tmp \
    && wget "https://www.php.net/distributions/php-${PHP_VERSION}.tar.gz" \
    && tar -xzf "php-${PHP_VERSION}.tar.gz" \
    && cd "php-${PHP_VERSION}" \
    && ./configure --enable-zts --disable-all \
    && make -j"$(nproc)" \
    && make install \
    && rm -rf "/tmp/php-${PHP_VERSION}" "/tmp/php-${PHP_VERSION}.tar.gz"

WORKDIR /workspace/crates/ffi/php

RUN rustup target add x86_64-unknown-linux-gnu
