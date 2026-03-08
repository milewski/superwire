clippy:
    cargo clippy --fix --allow-dirty
    cargo fmt

try:
    RUST_LOG=debug cargo run -p engine-ai-example