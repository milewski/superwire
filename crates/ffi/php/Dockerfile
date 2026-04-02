FROM rust:1.94-bookworm

ARG PHP_VERSION=8.4.19
ARG PHP_ZTS=0
ARG TARGET_TRIPLE=x86_64-unknown-linux-gnu

RUN apt update && apt install -y --no-install-recommends \
    clang build-essential musl-tools pkg-config wget ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN cd /tmp \
    && wget "https://www.php.net/distributions/php-${PHP_VERSION}.tar.gz" \
    && tar -xzf "php-${PHP_VERSION}.tar.gz" \
    && cd "php-${PHP_VERSION}" \
    && if [ "${PHP_ZTS}" = "1" ]; then ZTS_FLAG="--enable-zts"; else ZTS_FLAG="--disable-zts"; fi \
    && ./configure ${ZTS_FLAG} --disable-all \
    && make -j"$(nproc)" \
    && make install \
    && rm -rf "/tmp/php-${PHP_VERSION}" "/tmp/php-${PHP_VERSION}.tar.gz"

WORKDIR /workspace/crates/ffi/php

RUN rustup +nightly target add ${TARGET_TRIPLE}
