#!/usr/bin/env bash
set -euo pipefail

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
component_target="wasm32-unknown-unknown"
tool_sources_directory="$script_directory/tool-sources/src"
tool_output_directory="$script_directory/tools"
generated_tools_directory="$script_directory/target/tool-build"
shared_target_directory="$script_directory/target/tool-target"
wit_source_directory="$script_directory/../crates/core/wit"
tool_sdk_crate_directory="$script_directory/../crates/wasm-tool-sdk"

if ! cargo component --version >/dev/null 2>&1; then
  echo "cargo-component is required. Install with: cargo install cargo-component"
  exit 1
fi

if [ ! -d "$tool_sources_directory" ]; then
  echo "tool source directory not found: $tool_sources_directory"
  exit 1
fi

if [ ! -d "$wit_source_directory" ]; then
  echo "wit source directory not found: $wit_source_directory"
  exit 1
fi

if [ ! -d "$tool_sdk_crate_directory" ]; then
  echo "tool sdk crate directory not found: $tool_sdk_crate_directory"
  exit 1
fi

mkdir -p "$tool_output_directory"
mkdir -p "$generated_tools_directory"

shopt -s nullglob

tool_source_files=()

for tool_source_candidate in "$tool_sources_directory"/*.rs; do
  if [ "$(basename "$tool_source_candidate")" = "lib.rs" ]; then
    continue
  fi

  tool_source_files+=("$tool_source_candidate")
done

if [ ${#tool_source_files[@]} -eq 0 ]; then
  echo "no tool source files found in $tool_sources_directory"
  exit 1
fi

for tool_source_path in "${tool_source_files[@]}"; do
  tool_name="$(basename "$tool_source_path" .rs)"
  generated_tool_crate_directory="$generated_tools_directory/$tool_name"
  generated_tool_crate_source_directory="$generated_tool_crate_directory/src"

  rm -rf "$generated_tool_crate_directory"
  mkdir -p "$generated_tool_crate_source_directory"
  cp -R "$wit_source_directory" "$generated_tool_crate_directory/wit"

  cat > "$generated_tool_crate_directory/Cargo.toml" <<EOF
[package]
name = "${tool_name}-wasm-tool"
version = "0.1.0"
edition = "2021"

[lib]
name = "tool_component"
path = "src/lib.rs"
crate-type = ["cdylib"]

[dependencies]
schemars = "1.2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
wit-bindgen = "0.55"
superwire-wasm-tool-sdk = { path = "$tool_sdk_crate_directory" }

[workspace]
EOF

  cat > "$generated_tool_crate_source_directory/lib.rs" <<EOF
wit_bindgen::generate!({
    path: "wit",
    world: "superwire-tool",
});

mod user_tool {
    include!("$tool_source_path");
}

use exports::superwire::tool::tool::{Guest, ToolDefinition, ToolError};

fn host_http_get_delegate(request_url: &str) -> Result<String, String> {
    superwire::tool::host::http_get(request_url)
}

struct GeneratedTool;

impl Guest for GeneratedTool {
    fn definition() -> Result<ToolDefinition, String> {
        superwire_wasm_tool_sdk::host::register_http_get(host_http_get_delegate);

        let tool_definition = superwire_wasm_tool_sdk::build_tool_definition_json::<user_tool::Tool>()?;

        Ok(ToolDefinition {
            name: tool_definition.name,
            description: tool_definition.description,
            parameters_schema_json: tool_definition.parameters_schema_json,
            bound_parameters_schema_json: tool_definition.bound_parameters_schema_json,
            output_schema_json: tool_definition.output_schema_json,
        })
    }

    fn execute(agent_input_json: String, bound_input_json: String) -> Result<String, ToolError> {
        superwire_wasm_tool_sdk::host::register_http_get(host_http_get_delegate);

        superwire_wasm_tool_sdk::execute_tool_json::<user_tool::Tool>(&agent_input_json, &bound_input_json)
            .map_err(|error| ToolError {
                code: error.code,
                message: error.message,
            })
    }
}

export!(GeneratedTool);
EOF

  CARGO_TARGET_DIR="$shared_target_directory" cargo component build \
    --manifest-path "$generated_tool_crate_directory/Cargo.toml" \
    --release \
    --target "$component_target"

  cp \
    "$shared_target_directory/$component_target/release/tool_component.wasm" \
    "$tool_output_directory/$tool_name.wasm"

  echo "built $tool_output_directory/$tool_name.wasm"
done
