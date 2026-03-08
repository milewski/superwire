# AI Engine DSL Examples

This directory contains comprehensive examples demonstrating all features of the AI Engine DSL.

## Running Examples

### Run a single example:
```bash
cargo run --package engine-ai-example crates/example/workflows/01_basic_schema.engine.ai
```

### Run all examples:
```bash
./test_examples.sh
```

## Example Workflows

### 01_basic_schema.engine.ai
**Features:** Basic agent with schema validation, schema references
- Demonstrates schema definition with field descriptions
- Shows schema validation with union types (enums)
- Tests boolean, string types

### 02_agent_references.engine.ai
**Features:** Agent dependencies, template interpolation
- Shows how one agent can reference another agent's output
- Demonstrates {{ variable }} template syntax
- Tests dependency graph execution order

### 03_for_each_with_reference.engine.ai
**Features:** for_each iteration, agent output references
- Iterates over an array from another agent's output
- Uses binding variables in templates
- Returns array of results

### 04_context_summary.engine.ai
**Features:** Context summarization
- Demonstrates agent.name.context.summary syntax
- Shows lazy context summarization
- Tests context sharing between agents

### 05_multiple_terminals.engine.ai
**Features:** Multiple terminal agents
- Shows multiple agents marked with <- prefix
- Demonstrates final output as JSON object with multiple keys
- Tests independent agent execution

### 06_complex_schema.engine.ai
**Features:** Complex schema with multiple types
- Arrays of strings
- Union types (enums)
- Field descriptions
- Boolean, number, string types

### 07_dependency_chain.engine.ai
**Features:** Multi-step dependency chain
- Three agents in sequence
- Each agent depends on the previous one
- Tests topological ordering

### 08_for_each_inline_schema.engine.ai
**Features:** for_each with inline schema
- Inline schema definition (no schema name)
- Structured output from each iteration
- Tests schema validation in loops

### 09_file_function.engine.ai
**Features:** file() template function
- Reads external template file
- Variable substitution in templates
- Tests template validation

### 10_full_context.engine.ai
**Features:** Full context sharing
- Uses agent.name.context (not .summary)
- Shares complete message history
- Tests context isolation

## Prerequisites

- Ollama server running at http://100.76.5.36:11434
- Model qwen3.5:27b available

## Expected Behavior

All examples should:
1. Parse successfully
2. Validate without errors
3. Execute against Ollama
4. Return valid JSON output
5. Complete within reasonable time (30-60 seconds per workflow)

## Troubleshooting

If examples fail:
1. Check Ollama server is accessible: `curl http://100.76.5.36:11434/api/tags`
2. Verify model is available: `ollama list | grep qwen3.5:27b`
3. Check logs with: `RUST_LOG=info cargo run ...`
4. Review schema validation errors - LLM may need better prompts
