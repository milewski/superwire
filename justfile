clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

# Run the CLI with arguments
cli *arguments:
    cargo run --release -p superwire-cli -- {{arguments}}

# Build IntelliJ plugin (bundles LSP binaries)
intellij-build:
    cd editors/intellij && ./gradlew clean buildPlugin
