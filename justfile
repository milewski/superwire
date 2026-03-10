clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

try:
    RUST_LOG=debug cargo run -p engine-ai-example

# Run the CLI with arguments
cli *arguments:
    cargo run --release -p engine_ai_cli -- {{arguments}}

# Build a workflow to a standalone executable
build workflow output:
    cargo run --release -p engine_ai_cli -- build {{workflow}} --output {{output}}

# Example: compile the input_output workflow
build-example:
    cargo run --release -p engine_ai_cli -- build crates/test/workflows/input_output.ai --output ./compiled-workflow