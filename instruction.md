# AI Engine DSL Specification

## Overview

This document defines a domain-specific language (DSL) for describing agent workflows. A workflow consists of:

- `agent` nodes, which execute prompts against language models
- `schema` definitions, which define structured outputs
- `provider` definitions, which configure model backends
- dependency references, which determine execution order

DSL files use the `.ai` extension.

The parser must validate the document before execution. Invalid graphs, invalid references, and invalid template usage
must produce parse-time errors.

---

## Core Concepts

### Agent

An `agent` defines a unit of execution.

Example:

```txt
agent summary {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "Summarize the following text"
}
```

Each agent name must be unique within the graph. Duplicate agent names must raise a parse-time error.

### Supported Agent Properties

An agent may contain the following properties:

```txt
model <- string
tools <- [string, string, ...]
context <- string
output <- schema reference | inline schema
prompt <- string | multiline string | function call
for_each <- expression as identifier
```

### Agent Execution Loop

Each agent executes inside an agent loop.

Every agent includes a built-in `done` tool by default. The `done` tool is a system-level tool and does not need to be
declared explicitly in the agent's `tools` list.

The only way for an agent to exit the agent loop is by calling the `done` tool and providing its final output.

The `done` tool must accept two parameters:

- `status`: either "success" or "fail"
- `output`: the final output value (required for success) or error reason (required for fail)

If an agent defines an `output` schema, the value provided to `done` with status "success" must validate against that
schema.

If schema validation fails, the validation error must be returned to the agent, and the agent must continue running
inside the loop until it produces a valid output.

If an agent does not define an `output` schema, its final output is a plain string.

When an agent calls `done` with status "fail", the workflow execution should handle the failure appropriately (e.g.,
propagate the error, log it, or allow dependent agents to handle it based on the execution strategy).

### Context Isolation and Sharing

Each agent starts with its own clean context. Agents do not share message history or execution context by default.

If a workflow needs one agent to reuse the exact context of another agent, it must reference that agent's context
explicitly.

Example:

```txt
agent one {
    prompt <- "..."
}

agent two {
    context <- agent.one.context
}
```

In this example, `agent two` receives the exact same message history and context as `agent one`.

If the workflow needs a summary of another agent's context, it can use the `compact` function to generate one. The
`compact` function is covered in detail in the "Input and Output Blocks" section.

---

## Agent References and Dependencies

Agents may reference outputs from other agents.

Example:

```txt
agent one {
    output <- schema {
        name: string
    }
}

agent two {
    prompt <- "Hello {{ one.name }}"
}
```

A reference from one agent to another creates a dependency edge in the execution graph.

The engine must:

- build a dependency graph using `petgraph`
- reject cyclic dependencies at parse time
- execute agents in dependency order
- execute independent agents in parallel using `rayon`

If two agents do not depend on each other, they may be executed in parallel.

---

## Schemas

A `schema` defines the expected structure of an agent output.

Example:

```txt
schema person {
    name: string "The user name, must be in this format first_name last_name"
    age: number
    gender: "male" | "female"
    hobbies: [string]
    is_gamer: boolean
    nickname: string | null
}
```

Schemas are compiled into JSON Schema and used to validate agent outputs.

All JSON Schema definitions in the Rust implementation must be declared explicitly using the `schemars` type system and
schema-generation APIs. Do not manually construct schema representations using `serde_json::json!()` or other ad hoc
JSON value builders. Field types, nullability, enums, arrays, object structure, and descriptions must all be expressed
through `schemars` so the generated schemas remain strongly typed, consistent, and maintainable.

### Schema Field Descriptions

Schema fields may include optional string descriptions that document the field's purpose or constraints:

```txt
schema person {
    name: string "The user name, must be in this format first_name last_name"
    age: number "Age in years"
}
```

These descriptions should be included in the generated JSON Schema as `description` properties for the corresponding
fields. This allows the LLM to understand field requirements and constraints when generating structured outputs.

Example usage:

```txt
agent one {
    output <- schema.person
}
```

### Inline Schemas

Schemas may also be defined inline:

```txt
agent one {
    output <- schema {
        name: string
        age: number
        gender: "male" | "female"
        hobbies: [string]
        is_gamer: boolean
        nickname: string | null
    }
}
```

Inline schemas do not require a name.

### Supported Schema Types

The following schema types are supported:

- `string`
- `number`
- `boolean`
- `null`
- arrays: `[T]`
- enums: `A | B`

The output of a schema-validated agent is expected to be JSON compatible with the generated JSON Schema.

### Schema Injection into Agent Context

When an agent defines an `output` schema, the schema must be automatically injected into the agent's prompt context.
This ensures the agent understands the expected output format from the start, reducing token waste from trial-and-error
attempts.

The schema should be injected as part of the system instructions with clear formatting guidance. The injection should
include:

- The complete JSON Schema representation
- Clear instruction to return JSON following the schema
- Field descriptions and constraints from the schema definition

Example injection format:

```
You must return your response as JSON following this exact schema:

{schema_json_here}

Ensure your output is valid JSON that matches this structure.
```

This injection happens automatically during agent execution and is not visible in the DSL. The agent receives both its
configured prompt and the schema requirements in its context.

---

## Prompt Values

A `prompt` may be defined in one of three ways:

1. inline string
2. multiline string
3. function call that returns a string

Examples:

```txt
agent one {
    prompt <- "A single line prompt"
}

agent two {
    prompt <- """
        A multiline prompt
    """
}

agent three {
    prompt <- file "./prompts/one.md" {
        system <- "System instructions"
        field_a <- "Value for field a"
        field_b <- "Value for field b"
    }
}
```

---

## Template Functions

The built-in `file` function reads a file and performs variable substitution.

Example:

```txt
agent three {
    prompt <- file "./prompts/one.md" {
        system <- "System instructions"
        field_a <- "Value for field a"
    }
}
```

Template variables inside the file must use the form:

```txt
{{ variable_name }}
```

### Template Validation Rules

The parser must raise a parse-time error if:

- the template contains a variable that is not provided in the replacement block
- the replacement block contains a variable that does not appear in the template

This ensures templates and replacement bindings remain consistent.

### Nested Functions

Functions may be nested:

```txt
agent three {
    prompt <- file "./prompts/one.md" {
        system <- "System instructions"
        field_a <- file "./prompts/two.md" {
            subfield <- "Value for subfield"
        }
    }
}
```

Nested function calls should be resolved during parsing. Independent function calls may be evaluated in parallel. If any
function evaluation fails, parsing must abort and return an error.

---

## String Interpolation

Strings may reference variables using:

```txt
{{ variable_name }}
```

Variable interpolation occurs at runtime unless the value is statically known during parsing.

Example:

```txt
agent one {
    prompt <- """
        Hello {{ user_name }}
    """
}
```

---

## for_each

The `for_each` property executes an agent once for each element in a collection.

Example:

```txt
agent one {
    for_each <- [1, 2, 3] as index

    output <- schema {
        output: number
    }

    prompt <- """
        How much is {{ index }} * 5?
    """
}
```

Each iteration runs independently and may be executed in parallel.

### Result Shape

If an agent uses `for_each`, its final output is an array containing the output from each iteration, in iteration order.

Example:

```txt
agent hobbies {
    output <- schema {
        hobbies: [string]
    }

    prompt <- "List common hobbies"
}

<- agent activities {
    for_each <- hobbies.hobbies as hobby

    output <- schema {
        activities: [string]
    }

    prompt <- """
        Create a list of activities related to the hobby: {{ hobby }}
    """
}
```

In this example, `activities` returns an array of objects, one per hobby.

### Terminal Agent

An agent prefixed with `<-` is a terminal agent. The output of terminal agents becomes the final program output.

If exactly one terminal agent is declared, the final program output is that agent's output directly.

If multiple terminal agents are declared, the final program output is a JSON object whose keys are the terminal agent
names and whose values are their respective outputs.

If no terminal agents are declared, the workflow executes all agents but produces no final output.

---

## Input and Output Blocks

### Input Block

The `input` block defines external parameters that can be provided to the workflow at runtime. This allows workflows to
be parameterized and reused with different values.

Example:

```txt
input {
    user_name: string
    topic: string
}

agent research {
    model <- "ollama1/qwen3.5:27b"
    output <- {
        summary: string
        key_points: [string]
    }

    prompt <- "Research the topic: {{ input.topic }}"
}

agent personalize {
    model <- "ollama1/qwen3.5:27b"
    output <- {
        message: string
    }

    prompt <- """
        Create a personalized message for {{ input.user_name }} about this research:
        {{ research.summary }}
    """
}

output {
    user <- input.user_name
    research_summary <- research.summary
    personalized_message <- personalize.message
}
```

Input fields are referenced using the `input.` prefix (e.g., `input.user_name`, `input.topic`).

### Output Block

The `output` block defines the final structure of the workflow output. It allows grouping and transforming agent outputs
into a single structured result.

Example:

```txt
output {
    user <- input.user_name
    research_summary <- research.summary
    personalized_message <- personalize.message
}
```

Each field in the `output` block can reference:

- Agent outputs (e.g., `agent_name.field`)
- Agent context summaries (e.g., `agent_name.context.summary`)
- Full agent context (e.g., `agent_name.context`)
- Input values (e.g., `input.field_name`)
- Hardcoded values (e.g., `"static string"`)

### Referencing Agent Context in Output Block

The `output` block supports referencing both full agent context and generating summaries using the `compact` function:

```txt
output {
    requested_topic <- input.topic
    requested_audience <- input.audience
    person_name <- agent.collect_person.name
    context <- agent.collect_person.context
}
```

When referencing the full context using `agent_name.context`, the entire conversation history is returned as a
structured `serde_json::Value` object. This is NOT a string representation - it is the complete, structured message
history exactly as it was passed to the AI provider, including:

- All user messages
- All assistant responses
- All tool calls made by the agent
- All tool call results returned to the agent
- The complete message sequence in the exact format used by the provider

The context is returned as an array of message objects. Example structure:

```json
[
  {
    "type": "user",
    "content": "Research the topic of artificial intelligence"
  },
  {
    "type": "assistant",
    "content": "I'll research that topic for you.",
    "tool_calls": [
      {
        "id": "call_123",
        "name": "search",
        "arguments": "{\"query\": \"artificial intelligence\"}"
      }
    ]
  },
  {
    "type": "tool",
    "tool_call_id": "call_123",
    "content": "Search results: AI is the simulation of human intelligence..."
  },
  {
    "type": "assistant",
    "content": "Based on my research, artificial intelligence refers to..."
  }
]
```

Each message object contains a `type` field indicating the message role (user, assistant, or tool) and the relevant
content and metadata for that message type.

### Compact Function for Context Summarization

To generate a summary of one or more agent contexts, use the `compact` function. This function takes a model and a list
of contexts to summarize:

```txt
output {
    requested_topic <- input.topic
    requested_audience <- input.audience
    person_name <- agent.collect_person.name
    context <- agent.collect_person.context
    summary <- compact {
        model <- "ollama1/qwen3.5:27b"
        context <- [agent.collect_person.context]
    }
}
```

The `compact` function can also combine and summarize multiple agent contexts:

```txt
output {
    combined_summary <- compact {
        model <- "ollama1/qwen3.5:27b"
        context <- [agent.one.context, agent.two.context, agent.three.context]
    }
}
```

The `compact` function returns the same message structure as `agent.context`, making it fully compatible with any place
that accepts context. This means you can use `compact` output directly as the context for another agent:

```txt
agent summarize_person {
    model <- "ollama1/qwen3.5:27b"
    context <- compact {
        model <- "ollama1/qwen3.5:27b"
        context <- [agent.collect_person.context]
    }
    prompt <- "Summarize the generated person for a {{ input.audience }} audience in one short paragraph."
}
```

When providing a single context, you can omit the array syntax and pass the context directly:

```txt
agent summarize_person {
    model <- "ollama1/qwen3.5:27b"
    context <- compact {
        model <- "ollama1/qwen3.5:27b"
        context <- agent.collect_person.context
    }
    prompt <- "Summarize the generated person for a {{ input.audience }} audience in one short paragraph."
}
```

In these examples:

- `agent.collect_person.context` returns the complete message history as a `serde_json::Value` object containing all
  messages, tool calls, and responses in their structured form
- `compact` generates a summary by processing the provided contexts using the specified model
- `compact` returns the same message array structure as `agent.context`, making it compatible for use as agent context
- Multiple contexts can be passed as an array `[context1, context2]` or a single context can be passed directly
- The output of `compact` can be used anywhere that accepts context, including as the `context` property of another
  agent

The context object preserves the full fidelity of the agent's execution history and can be used for debugging, auditing,
or passing to other systems that need to understand the complete interaction.

### Merging Terminal Agents with Output Block

When both terminal agents (prefixed with `<-`) and an `output` block are present, their outputs are merged into a single
JSON object. Terminal agent outputs are included as keys with their agent names, and output block fields are included as
additional keys.

Example:

```txt
input {
    topic: string
}

<- agent summary {
    model <- "ollama1/qwen3.5:27b"
    output <- {
        text: string
    }

    prompt <- "Summarize: {{ input.topic }}"
}

<- agent keywords {
    model <- "ollama1/qwen3.5:27b"
    output <- {
        words: [string]
    }

    prompt <- "Extract keywords from: {{ input.topic }}"
}

output {
    topic <- input.topic
    timestamp <- "2026-03-08T10:00:00Z"
}
```

Expected output:

```json
{
  "topic": "artificial intelligence",
  "timestamp": "2026-03-08T10:00:00Z",
  "summary": {
    "text": "AI is the simulation of human intelligence..."
  },
  "keywords": {
    "words": [
      "artificial",
      "intelligence",
      "machine",
      "learning"
    ]
  }
}
```

In this example:

- The terminal agent `summary` contributes its output under the key `"summary"`
- The terminal agent `keywords` contributes its output under the key `"keywords"`
- The `output` block contributes `"topic"` and `"timestamp"` fields
- All fields are merged into a single JSON object

Example with multiple terminal agents:

```txt
<- agent list {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "create a list from 0 to 10"
}

<- agent atoz {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "spell out all letters from alphabet from a to z"
}
```

Since neither agent defines an `output` schema, their outputs are plain strings. The final program output is:

```json
{
  "list": "0,1,2,3,4,5,6,7,8,9,10",
  "atoz": "a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z"
}
```

Example with a single terminal agent:

```txt
<- agent list {
    model <- "ollama1/qwen3.5:27b"
    prompt <- "create a list from 0 to 10"
}
```

The final program output is:

```json
{
  "list": "0,1,2,3,4,5,6,7,8,9,10"
}
```

The output is always a JSON object with the agent name as the key, regardless of whether there is one or multiple
terminal agents.

## Providers

A `provider` configures access to an LLM backend.

Example:

```txt
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://100.76.5.36:11434"
    models <- ["qwen3.5:27b"]
}

provider ollama2 {
    driver <- "ollama"
    api_endpoint <- "http://123.1.1.1:11434"
    models <- ["qwen3.5:35b"]
}
```

Agents select models using the format:

```txt
provider_name/model_name
```

Example:

```txt
agent one {
    model <- "ollama1/qwen3.5:27b"
}

agent two {
    model <- "ollama2/qwen3.5:35b"
}
```

### Provider Validation Rules

The parser must raise a parse-time error if:

- an agent references a provider that does not exist
- an agent references a model that is not declared by the provider

---

## Parse-Time Errors

The parser must reject the document if any of the following occur:

- duplicate agent names
- duplicate schema names
- cyclic agent dependencies
- undefined agent references
- undefined schema references
- undefined provider references
- provider/model mismatches
- missing template variables
- unused template bindings
- invalid property names
- invalid property value types

---

## Error Reporting

Error messages must be user-friendly and provide precise information to help users quickly identify and fix issues.

### Requirements for Error Messages

All syntax and validation errors must include:

1. **File path**: The path to the `.ai` file containing the error
2. **Line number**: The exact line where the error occurred (1-indexed)
3. **Column number**: The exact column where the error occurred (1-indexed)
4. **Visual pointer**: A caret (`^`) or similar indicator pointing to the exact location of the error
5. **Error description**: A clear explanation of what went wrong
6. **Suggestion**: Actionable guidance on how to fix the error, including correct syntax examples when applicable

### Error Message Format

Error messages should follow this format:

```
Error: <error description>
  --> <file_path>:<line>:<column>
   |
<line_number> | <source code line>
   | <spaces><caret pointing to error location>
   |
   = help: <suggestion with correct syntax or fix>
```

### Example Error Messages

**Example 1: Syntax Error**

```
Error: expected '{' after agent name
  --> workflows/example.ai:5:15
   |
 5 | agent summary
   |               ^
   |
   = help: agent blocks must be followed by '{'. Did you mean: agent summary { ... }
```

**Example 2: Undefined Reference**

```
Error: undefined agent reference 'summarizer'
  --> workflows/example.ai:12:18
   |
12 |     prompt <- "{{ summarizer.output }}"
   |                   ^^^^^^^^^^
   |
   = help: agent 'summarizer' is not defined. Available agents: summary, research, personalize
```

**Example 3: Type Mismatch**

```
Error: expected string value, found array
  --> workflows/example.ai:8:15
   |
 8 |     model <- ["ollama1/qwen3.5:27b"]
   |              ^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: the 'model' property expects a string value. Did you mean: model <- "ollama1/qwen3.5:27b"
```

**Example 4: Missing Required Property**

```
Error: missing required property 'model'
  --> workflows/example.ai:3:1
   |
 3 | agent summary {
   | ^^^^^
   |
   = help: agents must specify a 'model' property. Add: model <- "provider/model_name"
```

**Example 5: Invalid Property Name**

```
Error: unknown property 'modell'
  --> workflows/example.ai:4:5
   |
 4 |     modell <- "ollama1/qwen3.5:27b"
   |     ^^^^^^
   |
   = help: unknown property 'modell'. Did you mean 'model'?
```

**Example 6: Cyclic Dependency**

```
Error: cyclic dependency detected
  --> workflows/example.ai:15:18
   |
15 |     prompt <- "{{ agent_a.output }}"
   |                   ^^^^^^^
   |
   = help: agent 'agent_b' depends on 'agent_a', but 'agent_a' also depends on 'agent_b' (directly or indirectly). Break the cycle by removing one of the dependencies.
```

### Implementation Guidelines

- Use the `pest` crate's built-in error reporting capabilities where possible
- For validation errors that occur after parsing, maintain span information from the parse tree to provide accurate
  location data
- When suggesting fixes, prioritize the most likely intended syntax based on context
- For typos in identifiers (agent names, property names), use string similarity algorithms (e.g., Levenshtein distance)
  to suggest corrections
- Group related errors when multiple issues exist, but report them individually with their specific locations
- Use color coding in terminal output (if supported) to highlight error locations and suggestions

---

## Execution Rules

1. Parse and validate the document
2. Build the dependency graph
3. Reject cyclic graphs
4. Resolve parse-time functions
5. Execute agents in topological order
6. Execute independent agents in parallel
7. Validate agent outputs against their schemas
8. Return the output of the terminal agent

---

## Implementation Notes

The implementation should follow these guidelines:

- Use `petgraph` for dependency graph construction and topological ordering.
- Use `rayon` for parallel execution of independent agents and `for_each` iterations.
- Compile schemas to JSON Schema before execution.
- Fail fast on validation and parsing errors.
- Use the `schemars` crate for schema generation and validation support.
- Use `serde_json` for handling JSON data.
- Use the `pest` crate to parse the DSL.
- Use the `tokio` crate for asynchronous execution of agents and function calls.
- Design the system to be extensible for future features such as conditionals, loops, and more complex data types.
- Implement `providers` using traits so new provider backends can be added later.
- Start with an Ollama provider implementation using the `ollama-rs` crate.
- A test Ollama server is available at `http://100.76.5.36:11434` with the models `qwen3:8b` and `qwen3.5:27b`. The
  implementation should be tested and debugged against this server to validate that the provider integration, agent
  execution, and tool calling work correctly in practice.
- The Ollama implementation must use `ollama_rs::coordinator::Coordinator` to add tools and maintain the agentic loop.
  Use the `coordinator.chat()` API for tool calling support. The `.generate()` API does not support tool calling and
  should not be used for agents that require tools.
- Use the `colog` and `log` crates for logging and debugging. Log important execution events including:
    - When an agent starts execution (log agent name, configured model, available tools)
    - Agent responses and outputs
    - Tool calls and their results
    - Schema validation attempts and results
- Handle errors using explicit error enums. Do NOT use `anyhow`. Each module should define its own error type that
  represents all possible errors from that module (e.g., `parser/error.rs`, `validation/error.rs`,
  `execution/error.rs`). This groups related errors together and makes error handling explicit and type-safe. Use the
  `thiserror` crate to reduce boilerplate when implementing error types.
- **Create comprehensive workflow examples in the example crate that cover all features of the DSL.** Each workflow
  should
  execute against the real Ollama server and validate that the implementation works correctly. There should be at least
  one example workflow for every DSL feature.

The core library should be organized into the following submodules:

- `parser`: parses the DSL and builds the internal representation.
- `validation`: validates the parsed graph, references, and schemas.
- `execution`: executes agents according to the execution rules.
- `providers`: implements model providers and their APIs.
- `schemas`: handles schema definitions, compilation, and validation.
- `utils`: contains shared utilities and helper functions.
- `tools`: handles tool definitions and tool execution.

Tool calling must use the native tool calling capabilities provided by the LLM provider. Do NOT instruct the model to
respond with special syntax (like XML tags or JSON blocks) and do NOT implement pattern matching or regex-based parsing
to extract tool calls from agent responses. Instead, use the provider's built-in tool calling API (e.g., Ollama's native
tool support) to define tools and parse tool invocations. This ensures reliability and compatibility with the provider's
expected behavior.

All Rust modules should use the `module/mod.rs` style for organization (e.g., `parser/mod.rs`, `validation/mod.rs`)
rather than single-file modules.

### Workspace Structure

The project should be organized as a Cargo workspace with three main crates:

- `crates/core`: the core implementation and reusable library. This crate should expose the parser, validator, execution
  engine, provider abstractions, and other public APIs that downstream projects can use.
- `crates/macros`: procedural macros used by the core crate and by consumers of the library. This crate should contain
  macros such as `#[tool]` and `#[provider]`.
- `crates/example`: an example application used to test, evaluate, and demonstrate the project in a realistic setup.

The workspace root should define these crates as members so they can be built, tested, and versioned together.

Example layout:

```txt
Cargo.toml
crates/
├── core/
├── macros/
└── example/
```

Example workspace configuration:

```toml
[workspace]
members = [
    "crates/core",
    "crates/macros",
    "crates/example",
]
```

To simplify extensibility and reduce boilerplate, define procedural macros such as `#[tool]` and `#[provider]` to
declare these entities in Rust and register them automatically.

It should also be possible to create testing macros that make parser and execution tests concise and readable.

Example:

```rust
let result = parser! {
    agent one {
        model <- "ollama/model"
    }
};

// Assertions go here
```

---

## Editor Support

### TextMate Syntax Highlighting for JetBrains IDEs

To enable syntax highlighting for `.ai` files in JetBrains IDEs (IntelliJ IDEA, WebStorm, PyCharm, etc.), create a
TextMate grammar bundle in the `editors/` directory.

The directory structure should be:

```txt
editors/
└── textmate
    ├── package.json
    └── syntaxes
        └── ai.tmLanguage.json
```

The TextMate grammar should read this DSL specification document to understand all directives, keywords, operators, and
syntax patterns, then create appropriate syntax highlighting rules for them. The grammar should use standard TextMate
scope names to ensure proper highlighting with JetBrains IDE themes.

Users should be able to install the bundle by adding the `editors/textmate` directory in their IDE's TextMate Bundles
settings.

Create a task list with ~20 tasks and start working on this project systematically, do not use git worktrees