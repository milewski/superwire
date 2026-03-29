#!/usr/bin/env bash

set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
package_root="${workspace_root}/crates/ffi/php"
prebuilt_root="${package_root}/native/prebuilt"

build_linux_gnu_x86_64() {
    local output_directory="${prebuilt_root}/linux-x86_64-gnu"

    mkdir -p "${output_directory}"

    cargo build -p ffi --release --features php-ext --target x86_64-unknown-linux-gnu

    cp "${workspace_root}/target/x86_64-unknown-linux-gnu/release/libffi.so" "${output_directory}/engine_ai_ffi.so"

    printf '%s\n' "Built linux-x86_64-gnu prebuilt binary"
}

build_linux_musl_x86_64() {
    local output_directory="${prebuilt_root}/linux-x86_64-musl"
    local musl_toolchain_root="${workspace_root}/.local-musl"
    local musl_path_prefix="${musl_toolchain_root}/toolchain"
    local musl_compiler="${musl_path_prefix}/x86_64-linux-musl-gcc"
    local musl_cpp_compiler="${musl_path_prefix}/x86_64-linux-musl-g++"
    local musl_libgcc_directory="${musl_toolchain_root}/alpine-libgcc/usr/lib"
    local musl_php_wrapper="${ENGINE_AI_FFI_PHP_MUSL_PHP:-$(command -v php || true)}"
    local musl_php_config_wrapper="${ENGINE_AI_FFI_PHP_MUSL_PHP_CONFIG:-$(command -v php-config || true)}"

    if [[ ! -x "${musl_compiler}" ]]; then
        printf '%s\n' "Skipping linux-x86_64-musl build: missing ${musl_compiler}"
        printf '%s\n' "Prepare the local musl toolchain first or set up custom wrapper paths in this script."

        return
    fi

    if [[ ! -x "${musl_php_wrapper}" || ! -x "${musl_php_config_wrapper}" ]]; then
        printf '%s\n' "Skipping linux-x86_64-musl build: missing PHP or php-config executable"
        printf '%s\n' "Resolved PHP=${musl_php_wrapper} PHP_CONFIG=${musl_php_config_wrapper}."

        return
    fi

    mkdir -p "${output_directory}"

    PATH="${musl_path_prefix}:$PATH" \
    PHP="${musl_php_wrapper}" \
    PHP_CONFIG="${musl_php_config_wrapper}" \
    CC_x86_64_unknown_linux_musl="${musl_compiler}" \
    CXX_x86_64_unknown_linux_musl="${musl_cpp_compiler}" \
    AR_x86_64_unknown_linux_musl="x86_64-linux-gnu-ar" \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${musl_compiler}" \
    PKG_CONFIG_ALLOW_CROSS=1 \
    RUSTFLAGS="-C target-feature=-crt-static -C link-self-contained=yes -C link-arg=-L${musl_libgcc_directory}" \
    cargo build -p ffi --release --features php-ext --target x86_64-unknown-linux-musl

    cp "${workspace_root}/target/x86_64-unknown-linux-musl/release/libffi.so" "${output_directory}/engine_ai_ffi.so"

    printf '%s\n' "Built linux-x86_64-musl prebuilt binary"
}

build_linux_gnu_x86_64
build_linux_musl_x86_64

printf '%s\n' "Finished building prebuilt matrix into ${prebuilt_root}"
