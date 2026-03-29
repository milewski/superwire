FROM rust:1.86-bookworm

ARG PHP_VERSION=8.4.19

ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf \
    bison \
    build-essential \
    ca-certificates \
    clang \
    composer \
    libxml2-dev \
    pkg-config \
    re2c \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN cd /tmp \
    && wget "https://www.php.net/distributions/php-${PHP_VERSION}.tar.gz" \
    && tar -xzf "php-${PHP_VERSION}.tar.gz" \
    && cd "php-${PHP_VERSION}" \
    && ./configure --enable-zts --disable-all \
    && make -j"$(nproc)" \
    && make install \
    && rm -rf "/tmp/php-${PHP_VERSION}" "/tmp/php-${PHP_VERSION}.tar.gz"

ENV COMPOSER_ALLOW_SUPERUSER=1

WORKDIR /workspace/crates/ffi/php

COPY docker/entrypoint.sh /usr/local/bin/engine-ai-php-entrypoint

RUN rustup toolchain install nightly

RUN chmod +x /usr/local/bin/engine-ai-php-entrypoint

ENTRYPOINT ["/usr/local/bin/engine-ai-php-entrypoint"]
CMD ["sleep", "infinity"]
