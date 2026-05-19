<p align="center"><img src="/documentation/docs/public/logo-vertical.svg" width="40%" alt="Superwire"></p>

<p align="center">
    <a href="#"><img src="https://github.com/milewski/superwire/actions/workflows/ci.yml/badge.svg" alt="Build Status"></a>
    <a href="#"><img src="https://img.shields.io/github/license/milewski/superwire" alt="License"></a>
</p>

# Superwire

A declarative DSL for building AI-powered workflows with type safety, composability, and runtime validation.

## Overview

Superwire provides a domain-specific language for defining AI agent workflows that can be parsed, validated, and executed programmatically. Workflows define providers, schemas, inputs, agents, and outputs in a single `.wire` file.

## Quick Start

### Run with Docker

```bash
docker run --rm -p 13703:13703 rmilewski/superwire:latest
```

Open [http://localhost:13703/playground](http://localhost:13703/playground) to access the Playground UI.

### Create a Workflow

Paste the following into the Playground editor and replace the provider values with your own:

```wire
provider openai from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: "your-api-key"
}

model openai_model from openai {
    id: "gpt-4"
}

agent greeting {
    model: model.openai_model
    instruction: "Write a short welcome message."
    output {
        message: string
    }
}

output {
    greeting: agent.greeting.message
}
```

## Core Concepts

Superwire workflows structure and orchestrate the execution of LLM agents. Define your agents, their models, and the data flow between them in a declarative `.wire` file—Superwire handles parsing, validation, and runtime execution.

**Advantages:**
- **Type safety** — Define inputs, outputs, and schemas with compile-time validation
- **Composability** — Reuse providers, models, and agents across workflows
- **Runtime validation** — Catch errors before they reach production
- **Portable** — Single `.wire` file contains everything needed to execute a workflow

For full documentation, visit [https://docs.superwire.dev](https://docs.superwire.dev).

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
