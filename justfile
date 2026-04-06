clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

# Run the CLI with arguments
cli *arguments:
    cargo run --release -p superwire-cli -- {{arguments}}

# Build a workflow to a standalone executable
build workflow output:
    cargo run --release -p superwire-cli -- build {{workflow}} --output {{output}}

# Example: compile the input_output workflow
build-example:
    cargo run --release -p superwire-cli -- build crates/test/workflows/input_output.ai --output ./compiled-workflow

# Build IntelliJ plugin (bundles LSP binaries)
intellij-build:
    cd editors/intellij && ./gradlew clean buildPlugin
