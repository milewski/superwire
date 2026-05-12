# Provider and Model Specification

This document defines the provider and model system for the DSL.

The design separates three concerns:

1. **Provider drivers** define how to talk to a backend implementation such as OpenAI, Ollama, Anthropic, or a custom runtime.
2. **Provider instances** configure access to one concrete backend, including endpoint, credentials, and provider-level options.
3. **Model profiles** define reusable model selections, including model ID, inference defaults, and model capabilities.

Agents do not call providers directly. Agents reference named model profiles.

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}

model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }
}

agent summarizer {
    model: model.fast
    instruction: "Summarize {{ input.text }}"
    output: string
}
```

---

## Design Goals

The provider and model syntax should make the runtime topology explicit.

```text
driver -> provider instance -> model profile -> agent
```

The language should avoid hiding this relationship behind magic function calls such as `openai("gpt-4.1-mini")`.

The syntax should optimize for these goals:

- Providers configure backend access.
- Models configure inference behavior.
- Agents describe task behavior.
- Reusable model profiles should avoid repeated model IDs and repeated inference settings.
- Per-agent model overrides should use the same block-extension style as tool bindings and other resource-specific configuration.
- The grammar should be declarative and statically analyzable.

---

## Provider Declarations

A provider declaration creates a named provider instance from a provider driver.

```wire
provider <name> from <driver> {
    <config_key>: <expression>
    ...
}
```

Example:

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}

provider local_ollama from ollama {
    endpoint: "http://localhost:11434"
}
```

Provider names must use `snake_case`.

Provider drivers are identifiers resolved by the runtime. Built-in drivers may include names such as:

```wire
openai
ollama
anthropic
```

Custom drivers may be registered by the host runtime and referenced the same way:

```wire
provider internal_llm from company_llm_gateway {
    endpoint: "https://llm.internal.example.com"
    api_key: secrets.internal_llm_api_key
}
```

A provider declaration does not select a model by itself. It only configures access to a backend.

### Provider Config Values

Provider config values are ordinary DSL expressions.

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
    organization: secrets.openai_organization_id
}
```

The runtime passes the config block to the provider driver.

The language reserves the provider declaration shape, but individual config keys are interpreted by the driver.

For example, the OpenAI driver may understand:

```wire
endpoint
api_key
organization
project
```

The Ollama driver may understand:

```wire
endpoint
```

The language parser should not require all drivers to share the same config keys.

---

## Model Declarations

A model declaration creates a named model profile from a provider instance.

```wire
model <name> from <provider> {
    id: <string>
    ...
}
```

Example:

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"
}

model smart from openai_cloud {
    id: "gpt-4.1"
}

model local_private from local_ollama {
    id: "qwen3.5:32b"
}
```

Model names must use `snake_case`.

A model declaration must contain an `id` field.

The `id` field is the provider-specific model identifier sent to the provider driver.

Model profiles are referenced through the `model` namespace:

```wire
model.fast
model.smart
model.local_private
```

Agents must reference a model profile. They must not provide raw model strings.

Valid:

```wire
agent summarizer {
    model: model.fast
    instruction: "Summarize {{ input.text }}"
    output: string
}
```

Invalid:

```wire
agent summarizer {
    model: "gpt-4.1-mini"
    instruction: "Summarize {{ input.text }}"
    output: string
}
```

Invalid:

```wire
agent summarizer {
    model: openai_cloud("gpt-4.1-mini")
    instruction: "Summarize {{ input.text }}"
    output: string
}
```

---

## Model Inference Defaults

Inference configuration belongs to the model profile by default.

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }
}
```

This means every agent using `model.fast` inherits these inference settings unless it overrides them at the usage site.

The `inference` block may contain provider-supported inference parameters such as:

```wire
temperature
top_p
max_tokens
seed
presence_penalty
frequency_penalty
stop
```

The DSL should not require every provider to support every inference key. Unsupported keys should be rejected by provider-specific validation when possible.

---

## Model Usage in Agents

Agents use model profiles through the `model` property.

```wire
agent classifier {
    model: model.fast
    instruction: "Classify {{ input.message }}"
    output: enum { bug, feature, question }
}
```

The simplest usage references the model directly.

```wire
model: model.fast
```

When an agent needs to customize the model profile for that specific usage, it may open a block after the model reference.

```wire
agent creative_writer {
    model: model.fast {
        inference {
            temperature: 0.8
            max_tokens: 8_000
        }
    }

    instruction: "Write a creative draft for {{ input.topic }}"
    output: string
}
```

This follows the same conceptual pattern as opening a block after a referenced tool or imported capability to provide local overrides.

The model usage block does not create a new named model profile. It creates an anonymous per-agent specialization of the referenced model.

---

## Model Override Semantics

Model usage overrides are shallow object merges by block type.

Given this model profile:

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        top_p: 1.0
        max_tokens: 4_000
    }
}
```

And this agent:

```wire
agent creative_writer {
    model: model.fast {
        inference {
            temperature: 0.8
        }
    }

    instruction: "Write creatively about {{ input.topic }}"
    output: string
}
```

The effective model configuration is:

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.8
        top_p: 1.0
        max_tokens: 4_000
    }
}
```

The agent override changes only `temperature`. It preserves `top_p` and `max_tokens` from the base model profile.

A model usage block may not change the provider.

Invalid:

```wire
agent invalid_agent {
    model: model.fast {
        provider: local_ollama
    }

    instruction: "..."
    output: string
}
```

A model usage block may not change the model `id` unless the language explicitly supports model ID overrides.

Recommended rule: model `id` should not be overridable at the agent usage site.

Invalid:

```wire
agent invalid_agent {
    model: model.fast {
        id: "gpt-4.1"
    }

    instruction: "..."
    output: string
}
```

If an agent needs a different model ID, define a separate named model profile.

```wire
model smart from openai_cloud {
    id: "gpt-4.1"
}
```

---

## Inference Override Precedence

Inference settings are resolved in this order, from lowest to highest precedence:

1. Provider driver defaults.
2. Provider instance defaults, if supported.
3. Model profile inference block.
4. Agent model usage inference block.

Example:

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}

model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }
}

agent draft_email {
    model: model.fast {
        inference {
            temperature: 0.5
        }
    }

    instruction: "Draft an email about {{ input.topic }}"
    output: string
}
```

Effective inference config:

```wire
inference {
    temperature: 0.5
    max_tokens: 4_000
}
```

---

## Provider-Level Defaults

Provider-level defaults are optional.

They are useful for settings that should apply to all models using that provider.

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key

    inference {
        timeout_seconds: 60
    }
}
```

Provider-level inference should be limited to settings that are genuinely provider-wide.

Recommended provider-level settings:

```wire
timeout_seconds
retry_attempts
retry_backoff
```

Recommended model-level settings:

```wire
temperature
top_p
max_tokens
seed
stop
```

The DSL should avoid putting task-specific behavior in providers.

---

## Model Capabilities

Model profiles may optionally declare capabilities.

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"

    capabilities {
        tools: true
        structured_output: true
        vision: false
    }
}
```

Capabilities may be used for static validation.

For example, an agent using tools requires a model whose effective capabilities include `tools: true`.

```wire
agent task_creator {
    model: model.fast
    uses: [tool.create_task]
    instruction: "Create a task from {{ input.message }}"
    output: schema.task
}
```

If `model.fast` declares `tools: false`, validation should fail.

Capabilities may be inferred by built-in provider drivers when possible. Explicit capability declarations are useful for custom providers or models unknown to the runtime.

---

## Model Fallbacks

Fallback behavior should be modeled explicitly rather than hidden inside provider config.

Recommended syntax:

```wire
model reliable_fast from openai_cloud {
    id: "gpt-4.1-mini"

    fallback: model.local_private
}
```

Multiple fallbacks may be represented as an ordered list:

```wire
model reliable_fast from openai_cloud {
    id: "gpt-4.1-mini"

    fallback: [model.smart, model.local_private]
}
```

Fallbacks should preserve the original agent instruction, context, tools, and output schema.

Fallback model compatibility should be validated statically when possible.

For example, if an agent requires tool calling, every fallback model must support tool calling.

---

## Complete Example

```wire
secrets {
    openai_api_key: string
}

input {
    message: string
}

provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key

    inference {
        timeout_seconds: 60
        retry_attempts: 2
    }
}

provider local_ollama from ollama {
    endpoint: "http://localhost:11434"
}

model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }

    capabilities {
        tools: true
        structured_output: true
    }
}

model smart from openai_cloud {
    id: "gpt-4.1"

    inference {
        temperature: 0.1
        max_tokens: 12_000
    }

    capabilities {
        tools: true
        structured_output: true
    }
}

model private from local_ollama {
    id: "qwen3.5:32b"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }

    capabilities {
        tools: false
        structured_output: true
    }
}

schema response {
    category: enum { bug, feature, question }
    summary: string
    confidence: number
}

agent classify_message {
    model: model.fast {
        inference {
            temperature: 0.0
        }
    }

    instruction: "Classify this user message: {{ input.message }}"
    output: schema.response
}

output {
    result: agent.classify_message
}
```

---

## Invalid Examples

### Raw model string in agent

```wire
agent invalid_agent {
    model: "gpt-4.1-mini"
    instruction: "..."
    output: string
}
```

Agents must reference a named model profile.

---

### Provider call in agent

```wire
agent invalid_agent {
    model: openai_cloud("gpt-4.1-mini")
    instruction: "..."
    output: string
}
```

Providers are not callable from agent declarations.

---

### Model without provider

```wire
model fast {
    id: "gpt-4.1-mini"
}
```

Models must declare which provider instance they use.

---

### Provider without driver

```wire
provider openai_cloud {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}
```

Providers must declare which driver they use.

---

### Agent override changing model ID

```wire
agent invalid_agent {
    model: model.fast {
        id: "gpt-4.1"
    }

    instruction: "..."
    output: string
}
```

Agent-level model usage blocks may override inference settings, but should not change model identity.

---

## Grammar Sketch

This section is intentionally approximate and should be adapted to the parser implementation.

```ebnf
provider_decl = "provider" ident "from" ident block ;

model_decl = "model" ident "from" ident model_block ;

model_block = "{" model_field* "}" ;

model_field =
      "id" ":" string
    | "inference" block
    | "capabilities" block
    | "fallback" ":" model_ref_or_list
    | custom_model_field ;

agent_decl = "agent" ident "{" agent_field* "}" ;

agent_field =
      "model" ":" model_usage
    | "instruction" ":" string
    | "output" ":" type_expr
    | "uses" ":" list_expr
    | "context" block
    | other_agent_field ;

model_usage = model_ref model_usage_block? ;

model_usage_block = "{" model_usage_field* "}" ;

model_usage_field =
      "inference" block
    | "fallback" ":" model_ref_or_list ;

model_ref = "model" "." ident ;
```

---

## Recommended Validation Rules

The compiler should validate:

1. Provider names are unique.
2. Model names are unique.
3. Provider declarations reference registered provider drivers.
4. Model declarations reference existing provider instances.
5. Every model declaration has an `id`.
6. Every agent has a `model`.
7. Agent `model` values reference existing model profiles.
8. Agent model usage blocks may override inference but may not override provider or model ID.
9. Unsupported inference keys should be rejected when the provider driver exposes a schema.
10. Agents using tools require a model that supports tools, if capabilities are known.
11. Agents using structured output require a model that supports structured output, if capabilities are known.
12. Fallback models must be compatible with the agent features they may need to execute.

---

## Recommended Style

Use provider names that describe deployment or location:

```wire
provider openai_cloud from openai
provider local_ollama from ollama
provider internal_gateway from company_llm_gateway
```

Use model names that describe role, cost, speed, or capability:

```wire
model fast
model smart
model cheap
model private
model long_context
model vision
model tool_capable
```

Avoid naming model profiles after raw provider IDs unless the exact ID matters to the workflow.

Prefer this:

```wire
model fast from openai_cloud {
    id: "gpt-4.1-mini"
}
```

Over this:

```wire
model gpt_4_1_mini from openai_cloud {
    id: "gpt-4.1-mini"
}
```

The first version communicates intent. The second version merely repeats implementation detail.

---

## Summary

Providers configure backend access.

Models define reusable inference profiles.

Agents reference model profiles and may locally override model behavior by opening a block after the model reference.

Canonical form:

```wire
provider openai_cloud from openai {
    endpoint: "https://api.openai.com/v1"
    api_key: secrets.openai_api_key
}

model fast from openai_cloud {
    id: "gpt-4.1-mini"

    inference {
        temperature: 0.2
        max_tokens: 4_000
    }
}

agent summarizer {
    model: model.fast {
        inference {
            temperature: 0.0
        }
    }

    instruction: "Summarize {{ input.text }}"
    output: string
}
```

This keeps provider configuration, model behavior, and agent task logic cleanly separated while preserving a concise override syntax for agent-specific tuning.
