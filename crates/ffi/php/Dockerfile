FROM rust:1.86-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    clang \
    composer \
    git \
    libclang-dev \
    libssl-dev \
    php-cli \
    php-curl \
    php-dev \
    php-mbstring \
    php-xml \
    pkg-config \
    unzip \
    && rm -rf /var/lib/apt/lists/*

ENV COMPOSER_ALLOW_SUPERUSER=1

WORKDIR /workspace/crates/ffi/php

COPY docker/entrypoint.sh /usr/local/bin/engine-ai-php-entrypoint

RUN chmod +x /usr/local/bin/engine-ai-php-entrypoint

ENTRYPOINT ["/usr/local/bin/engine-ai-php-entrypoint"]
CMD ["sleep", "infinity"]
