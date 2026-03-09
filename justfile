clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

try:
    RUST_LOG=debug cargo run -p engine-ai-example