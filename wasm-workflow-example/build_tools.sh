#!/usr/bin/env bash
set -euo pipefail

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wasm_target="wasm32-unknown-unknown"

rustup target add "$wasm_target"

cargo build --manifest-path "$script_directory/Cargo.toml" -p weather-wasm-tool --release --target "$wasm_target"

mkdir -p "$script_directory/tools"

cp \
  "$script_directory/target/$wasm_target/release/weather_wasm_tool.wasm" \
  "$script_directory/tools/weather.wasm"

echo "built $script_directory/tools/weather.wasm"
