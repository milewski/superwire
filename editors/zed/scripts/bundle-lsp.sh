#!/usr/bin/env bash
set -euo pipefail

profile="release"

if [[ $# -gt 0 ]]; then
  profile="$1"
fi

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "${script_directory}/../../.." && pwd)"
extension_root="$(cd "${script_directory}/.." && pwd)"

operating_system="$(uname -s)"
architecture="$(uname -m)"

case "${operating_system}" in
  Linux)
    operating_system_directory="linux"
    ;;
  Darwin)
    operating_system_directory="macos"
    ;;
  *)
    echo "Unsupported operating system: ${operating_system}" >&2
    exit 1
    ;;
esac

case "${architecture}" in
  x86_64)
    architecture_directory="x86_64"
    ;;
  aarch64|arm64)
    architecture_directory="aarch64"
    ;;
  *)
    echo "Unsupported architecture: ${architecture}" >&2
    exit 1
    ;;
esac

manifest_path="${repository_root}/crates/lsp/Cargo.toml"
cargo_arguments=(build --manifest-path "${manifest_path}" --bin superwire-lsp)

if [[ "${profile}" == "release" ]]; then
  cargo_arguments+=(--release)
elif [[ "${profile}" != "debug" ]]; then
  echo "Invalid profile '${profile}'. Use 'debug' or 'release'." >&2
  exit 1
fi

echo "Building superwire-lsp (${profile})..."
cargo "${cargo_arguments[@]}"

compiled_binary="${repository_root}/target/${profile}/superwire-lsp"

if [[ ! -f "${compiled_binary}" ]]; then
  echo "Compiled binary not found at ${compiled_binary}" >&2
  exit 1
fi

bundle_output_directory="${extension_root}/bin/${operating_system_directory}-${architecture_directory}"
mkdir -p "${bundle_output_directory}"

bundle_output_binary="${bundle_output_directory}/superwire-lsp"
cp "${compiled_binary}" "${bundle_output_binary}"
chmod +x "${bundle_output_binary}"

echo "Bundled binary written to ${bundle_output_binary}"
