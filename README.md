<p align="center"><img src="/documentation/public/logo-vertical.svg" width="40%" alt="Superwire"></p>

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

### Run with Docker

```yaml
services:
  superwire:
    image: rmilewski/superwire:latest
    ports:
      - 13703:13703
```

Open [http://localhost:13704](http://localhost:13704) to access the Playground UI.

### Create a Workflow

Create a `hello.wire` file:

```wire
provider openai from openai {
    endpoint: "http://localhost:1234/v1"
    api_key: "test-api-key"
}

model openai_model from openai {
    id: "gpt-5.5"
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

### Providers

Define AI model providers (Ollama, OpenAI, etc.):

```wire
provider openai from openai {
    endpoint: "https://olama.com/v1"
    api_key: secrets.OLLAMA_API_KEY
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
schema user {
    name: string
    age: number
    email: string
}
```

### Agents

Define AI agents with prompts and outputs:

```wire
agent greet_user {
    model: model.openai_gpt_4
    instruction: "Say hello to {{ input.name }}"
    output {
        message: string
    }
}
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
