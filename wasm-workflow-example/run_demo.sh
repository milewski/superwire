#!/usr/bin/env bash
set -euo pipefail

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

bash "$script_directory/build_tools.sh"

cd "$script_directory"

cargo run --manifest-path "$script_directory/Cargo.toml" -p wasm-workflow-host
