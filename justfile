clippy:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty -- -D warnings
    cargo fmt

# Run the CLI with arguments
cli *arguments:
    cargo run --release -p superwire-cli -- {{arguments}}

# Build IntelliJ plugin (bundles LSP binaries)
intellij-build:
    cd editors/intellij && JAVA_HOME=$HOME/.local/share/mise/installs/java/21.0.2 ./gradlew clean buildPlugin

build-cli-alpine:
    cargo build -p superwire-cli --release --target x86_64-unknown-linux-musl
    mv target/x86_64-unknown-linux-musl/release/superwire-cli ./superwire-cli

# Build the executor Docker image
build-docker tag="latest":
    docker build -t rmilewski/superwire:{{tag}} -f Dockerfile .

playground:
    cargo run --release -p superwire-executor

# Commit and push all submodules, then commit and push the main repo
# Usage: just submodule-push "commit message"
submodule-push commit_message:
    git submodule foreach "git add -A && git commit -m '{{commit_message}}' || true && git push -u origin HEAD"
    git add -A
    git commit -m "{{commit_message}}" || true
    git push origin HEAD

# Pull the main repo and all submodules
submodule-pull:
    git submodule update --init
    git pull origin HEAD
    git submodule foreach git pull origin HEAD
