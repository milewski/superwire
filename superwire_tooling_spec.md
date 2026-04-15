# Superwire Tooling Architecture Specification

## Overview

This document defines the architecture for Superwire's cross-platform tool system.

The goal is to establish a **portable, self-describing tool interface** that can be:

- Executed by the Superwire runtime
- Distributed as a single artifact
- Embedded in Rust
- Invoked externally from environments like PHP/Laravel

---

## Core Design Principles

### 1. Single Artifact

Each tool is distributed as a single:

```
.wasm (WebAssembly Component)
```

This artifact must contain:

- Execution logic
- Interface definition
- Metadata (self-describing)

No external `.json` or manifest files are required.

---

### 2. Self-Describing Tools

Each tool exposes metadata through an introspection API.

This enables:

- CLI inspection
- Runtime validation
- Dynamic registration

---

### 3. Runtime Ownership

The Superwire runtime is responsible for:

- Tool discovery
- Execution orchestration
- Validation
- Capability enforcement

Tools remain **pure execution units**.

---

### 4. Transport-Agnostic Execution

Tools can be executed via:

- Native (Rust)
- Wasm runtime (Wasmtime)
- CLI bridge (for PHP)
- Future adapters (HTTP, etc.)

---

## WIT Interface Specification

### Package

```wit
package superwire:tool@0.1.0;
```

---

### Execution Interface

```wit
interface tool {
  execute: func(input-json: string, bound-input-json: string) -> result<string, string>;
}
```

#### Notes

- `input-json`: raw input from DSL
- `bound-input-json`: resolved inputs after interpolation/binding
- return:
  - `Ok(string)` = JSON output
  - `Err(string)` = error message

---

### Introspection Interface

```wit
interface introspection {
  describe: func() -> string;
}
```

---

### World Definition

```wit
world superwire-tool {
  export tool;
  export introspection;
}
```

---

## Descriptor Schema

The `describe()` method must return JSON:

```json
{
  "schema_version": "superwire.tool.v1",
  "name": "weather",
  "version": "1.0.0",
  "description": "Get current weather",
  "input_schema": {},
  "bound_input_schema": {},
  "output_schema": {},
  "annotations": {
    "idempotent": true
  }
}
```

---

## Rust Runtime Architecture

### Core Trait

```rust
trait ToolBackend {
    fn execute(&self, input: String, bound_input: String) -> Result<String, String>;
    fn describe(&self) -> Result<String, String>;
}
```

---

### Implementations

#### 1. Wasm Backend

- Uses Wasmtime
- Loads component
- Calls:
  - `describe()`
  - `execute()`

#### 2. Native Backend

- Rust struct implementing trait directly

#### 3. CLI Backend

- Spawns process
- Communicates via stdin/stdout

---

## CLI Tooling

### Inspect

```
superwire inspect ./tools/weather.wasm
```

### Run

```
superwire run ./tools/weather.wasm --input '{}'
```

---

## PHP / Laravel Integration Strategy

### Approach

Use CLI runner:

- PHP calls CLI
- CLI executes Wasm
- JSON in/out

### Flow

```
Laravel → CLI → Wasm Runtime → Result
```

---

## Task Breakdown

### Phase 1: Core Runtime

- [ ] Define WIT interface
- [ ] Implement Wasm loader
- [ ] Implement execution bridge
- [ ] Implement describe() call
- [ ] JSON validation layer

---

### Phase 2: Tool Registry

- [ ] Tool discovery from folder
- [ ] Registry mapping
- [ ] Lazy loading
- [ ] Caching

---

### Phase 3: CLI

- [ ] `inspect` command
- [ ] `run` command
- [ ] error handling
- [ ] JSON parsing

---

### Phase 4: PHP Integration

- [ ] CLI wrapper package
- [ ] Laravel service binding
- [ ] Tool invocation API

---

### Phase 5: Advanced Features

- [ ] Capability restrictions
- [ ] Timeout handling
- [ ] Memory limits
- [ ] Sandboxing policies

---

## Key Considerations

### Security

- Default: no IO access
- No network unless explicitly allowed
- No filesystem access

---

### Determinism

Prefer:

- Pure functions
- No side effects

---

### Performance

- Cache compiled Wasm modules
- Avoid repeated instantiation

---

### Versioning

- `schema_version` required
- Backward compatibility via JSON

---

## Objectives

- Cross-language tool portability
- Minimal packaging complexity
- Strong runtime control
- Clean developer experience

---

## Non-Goals (for v1)

- Full workflow compilation to Wasm
- Distributed execution
- Tool networking model

---

## Summary

Superwire tools are:

- Self-contained
- Portable
- Declarative via WIT
- Executed through a unified runtime interface

The system balances:

- Flexibility (multiple backends)
- Portability (Wasm)
- Simplicity (single artifact)

