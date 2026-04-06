clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

# Run the CLI with arguments
cli *arguments:
    cargo run --release -p superwire-cli -- {{arguments}}

# Build IntelliJ plugin (bundles LSP binaries)
intellij-build:
    cd editors/intellij && ./gradlew clean buildPlugin

# Bundle Superwire LSP for Zed on Windows x86_64
zed-bundle-lsp-windows:
    cargo build --manifest-path crates/lsp/Cargo.toml --bin superwire-lsp --target x86_64-pc-windows-msvc --release
    powershell -NoProfile -Command "New-Item -ItemType Directory -Force editors/zed/bin/windows-x86_64 | Out-Null; Copy-Item -Force target/x86_64-pc-windows-msvc/release/superwire-lsp.exe editors/zed/bin/windows-x86_64/superwire-lsp.exe"

# Bundle Superwire LSP for Zed on Linux x86_64
zed-bundle-lsp-linux:
    cargo build --manifest-path crates/lsp/Cargo.toml --bin superwire-lsp --release
    mkdir -p editors/zed/bin/linux-x86_64
    cp target/release/superwire-lsp editors/zed/bin/linux-x86_64/superwire-lsp

# Bundle Superwire LSP for Zed on macOS arm64
zed-bundle-lsp-macos:
    cargo build --manifest-path crates/lsp/Cargo.toml --bin superwire-lsp --release
    mkdir -p editors/zed/bin/macos-aarch64
    cp target/release/superwire-lsp editors/zed/bin/macos-aarch64/superwire-lsp
