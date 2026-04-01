#!/usr/bin/env bash

set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
package_root="${workspace_root}/crates/ffi/php"
output_root="${package_root}/dist/prebuilt"

php_version="${PHP_VERSION:-8.4.19}"
php_zts="${PHP_ZTS:-0}"
target_triple="${TARGET_TRIPLE:-x86_64-unknown-linux-gnu}"
docker_platform="${DOCKER_PLATFORM:-linux/amd64}"

builder_tag="engine-ai-ffi-php-builder:${php_version}-${target_triple}-zts${php_zts}"

mkdir -p "${output_root}"

docker build \
    --platform "${docker_platform}" \
    --build-arg "PHP_VERSION=${php_version}" \
    --build-arg "PHP_ZTS=${php_zts}" \
    --build-arg "TARGET_TRIPLE=${target_triple}" \
    -f "${package_root}/Dockerfile" \
    -t "${builder_tag}" \
    "${workspace_root}"

docker run --rm \
    --platform "${docker_platform}" \
    -v "${workspace_root}:/workspace" \
    -w /workspace/crates/ffi/php \
    -e ENGINE_AI_FFI_PHP_WORKSPACE_ROOT=/workspace \
    -e ENGINE_AI_FFI_PHP_TARGET_TRIPLE="${target_triple}" \
    "${builder_tag}" \
    bash -lc 'php scripts/build-native.php && php scripts/package-prebuilt.php'

case "${target_triple}" in
    x86_64-unknown-linux-gnu)
        base_platform_key="linux-x86_64-gnu"
        ;;
    x86_64-unknown-linux-musl)
        base_platform_key="linux-x86_64-musl"
        ;;
    aarch64-unknown-linux-gnu)
        base_platform_key="linux-aarch64-gnu"
        ;;
    aarch64-unknown-linux-musl)
        base_platform_key="linux-aarch64-musl"
        ;;
    *)
        echo "Unsupported target triple for packaging: ${target_triple}" >&2
        exit 1
        ;;
esac

php_minor="${php_version%.*}"
threading_variant="nts"

if [[ "${php_zts}" == "1" ]]; then
    threading_variant="zts"
fi

runtime_platform_key="${base_platform_key}-php${php_minor}-${threading_variant}"

binary_source_path="${package_root}/native/prebuilt/${runtime_platform_key}/engine_ai_ffi.so"
checksum_source_path="${binary_source_path}.sha256"
binary_output_path="${output_root}/engine_ai_ffi-${runtime_platform_key}.so"
checksum_output_path="${binary_output_path}.sha256"

cp "${binary_source_path}" "${binary_output_path}"
cp "${checksum_source_path}" "${checksum_output_path}"

echo "Built prebuilt artifact ${binary_output_path}"
echo "Built checksum artifact ${checksum_output_path}"
