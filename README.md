<p align="center"><img src="/documentation/public/logo-vertical.svg" width="20%" alt="Superwire"></p>

<p align="center">
    <a href="#"><img src="https://github.com/milewski/superwire/workflows/ci/badge.svg" alt="Build Status"></a>
    <a href="#"><img src="https://img.shields.io/packagist/dt/milewski/superwire" alt="Total Downloads"></a>
    <a href="#"><img src="https://img.shields.io/packagist/v/milewski/superwire" alt="Latest Stable Version"></a>
    <a href="#"><img src="https://img.shields.io/packagist/l/milewski/superwire" alt="License"></a>
</p>

# Superwire

A declarative DSL for building AI-powered workflows with type safety, composability, and runtime validation.

## Overview

Superwire provides a domain-specific language for defining AI agent workflows that can be parsed, validated, and executed programmatically. Workflows define providers, schemas, inputs, agents, and outputs in a single `.wire` file.

## Quick Start

### Installation

```bash
# Install the CLI tool
cargo install --path crates/cli

# Or build from source
git clone https://github.com/milewski/superwire.git
cd superwire
cargo build --release
```

### Basic Workflow

Create a `example.wire` file:

```wire
provider ollama from ollama {
endpoint: "http://localhost:11434"
}

model ollama_model from ollama {
    id: "llama2"
}

schema Summary {
    text: string "The summary text"
    length: number "Character count"
}

input {
    content: string "Text to summarize"
}

agent summarizer {
    model: model.ollama_model
    
    prompt: """
        Summarize the following text in one sentence:
        {{ input.content }}
    """
    
    output: schema.Summary
}

output {
    result: agent.summarizer
}
```

### Run with CLI

```bash
# Format a workflow file
cli fmt example.wire

# Execute a workflow (via runtime)
cargo run --bin superwire-core --example minimum
```

## Core Concepts

### Providers

Define AI model providers (Ollama, OpenAI, etc.):

```wire
provider openai from openai {
api_key: secrets.OPENAI_KEY
}

model openai_gpt_4 from openai {
    id: "gpt-4"
}

model openai_gpt_3_5_turbo from openai {
    id: "gpt-3.5-turbo"
}
```

### Schemas

Define structured output types:

```wire
schema User {
    name: string "User's full name"
    age: number "Age in years"
    email: string "Email address"
}
```

### Agents

Define AI agents with prompts and outputs:

```wire
agent greet_user {
    model: model.openai_gpt_4
    
    prompt: "Say hello to {{ input.name }}"
    
    output: string
}
```

### Context Sharing

Agents can share context:

```wire
agent second_agent {
    model: model.openai_gpt_4
    context: context(agent.first_agent)
    
    prompt: "Continue from previous response"
    output: string
}
```

## Architecture

```
superwire/
├── crates/
│   ├── agent/      # Agent execution and tool runtime
│   ├── cli/        # Command-line interface
│   ├── core/       # DSL parser, validator, and runtime
│   ├── ffi/        # Foreign function interfaces (PHP, JavaScript)
│   └── lsp/        # Language server for editor support
├── editors/
│   ├── intellij/   # IntelliJ/JetBrains plugin
│   └── textmate/   # TextMate grammar for syntax highlighting
└── documentation/  # Mintlify documentation source
```

## Development

### Prerequisites

- Rust 1.80+
- Node.js 18+ (for JavaScript FFI)
- PHP 8.2+ (for PHP FFI, optional)

### Build

```bash
# Build all crates
cargo build --all

# Run tests
cargo test --all

# Run linter
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

### IntelliJ Plugin Development

```bash
cd editors/intellij
./gradlew buildPlugin
```

## Documentation

Full documentation is available at the [Superwire Docs](https://superwire.dev) (built from `documentation/` directory).

### Local Docs Preview

```bash
cd documentation
npx mintlify dev
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes with clear messages
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style

- Follow existing code patterns and conventions
- Run `cargo clippy` and `cargo fmt` before committing
- Add tests for new features
- Update documentation as needed

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

- Issues: [GitHub Issues](https://github.com/milewski/superwire/issues)
- Discussions: [GitHub Discussions](https://github.com/milewski/superwire/discussions)
