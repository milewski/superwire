# Workflow Runtime Long-Term Vision

## Purpose

This document captures the long-term architectural direction for the workflow runtime so future changes remain coherent, type-safe, and maintainable as the project grows.

The goal is to prevent early abstractions from becoming long-term constraints.

## Vision Statement

Build a statically typed workflow platform where:

1. Workflow correctness is validated as early as possible.
2. Runtime execution is deterministic and schema-constrained.
3. Diagnostics are precise, readable, and reusable across CLI, LSP, and tooling.
4. Language bindings (FFI) can integrate via stable, versioned interfaces.

## Strategic Principles

### 1) One semantic pipeline, reused everywhere

The same semantic compiler pipeline should power:

- CLI parsing/validation
- Runtime preflight
- LSP diagnostics and semantic features
- Future proc-macro compile-time checks

Reasoning:

- Prevents semantic drift between runtime, editor tooling, and compile-time checks.
- Reduces duplicate bug surfaces and maintenance overhead.

### 2) AST is syntax-facing, IR is execution-facing

The parser AST should preserve source-level constructs. A typed intermediate representation (IR) should represent resolved semantics for execution.

Reasoning:

- AST evolves with grammar.
- Runtime stability depends on semantic invariants, not syntax details.
- LSP can use typed IR for hover/go-to-definition/completion.

### 3) Diagnostics are a first-class product

Every subsystem should emit a shared diagnostic structure with:

- stable error code
- severity
- primary span
- secondary labels
- notes/help

Reasoning:

- Enables rich code-frame rendering (line/column arrows and snippets).
- Supports machine-readable diagnostics for LSP and external tooling.
- Keeps error quality consistent across parser, validator, and runtime.

### 4) Runtime does execution, compiler does correctness

Runtime should avoid semantic guessing or correction. All correctness checks should already be represented in typed plan/schema constraints.

Reasoning:

- Deterministic behavior.
- Strong guarantees for integrations.
- Easier debugging and incident analysis.

### 5) Stable boundary for FFI

Expose a stable service boundary (compile/validate/execute) through a versioned API, not internal Rust data structures.

Reasoning:

- Internal refactors remain safe.
- External language bindings stay compatible.

## Current Project Structure (Context Snapshot)

Top-level relevant crates and modules:

- `crates/core`
  - `src/dsl`: parser, AST, validation, macro entrypoints
  - `src/runtime`: workflow execution bridge and typed runtime logic
- `crates/agent`
  - loop executor, provider abstraction, tool runtime, finalize schema validation
- `crates/lsp`
  - language server integration (currently separate surface)

## Current Decisions Already In Place

These are active decisions that future work should preserve unless intentionally replaced:

1. Strict output-shape enforcement for agent results (no auto-recovery/casting).
2. Finalize tool answer schema can be constrained by runtime-provided schema.
3. Provider drivers use enum-based modeling (instead of uncontrolled string branching).
4. Runtime inference settings were refactored into typed enum keys and helper logic.
5. Workflow fragments can be composed in `parse_inline_workflow!` via `#fragment;` includes.

## Target Architecture (North Star)

1. **Parser Layer**
   - Produces CST/AST with stable spans.
2. **Semantic Compiler Layer**
   - Resolves symbols, types, and declarations.
   - Produces typed IR + diagnostics.
3. **Planning Layer**
   - Produces an `ExecutionPlan` (topological order, provider bindings, typed edges).
4. **Runtime Layer**
   - Executes only against `ExecutionPlan`.
   - Enforces schema contracts at IO boundaries.
5. **Tooling Layer**
   - LSP and CLI both consume shared diagnostics and semantic data.
6. **Interop Layer**
   - Versioned API for FFI and external tool integration.

## Non-Goals (For Scope Discipline)

- Do not add runtime heuristics that reinterpret incorrect outputs.
- Do not duplicate parser/type logic in LSP or FFI adapters.
- Do not expose unstable internal structs directly to external bindings.

## Risks If Not Addressed Early

1. Divergent semantics between runtime and editor tooling.
2. Hard-to-maintain error behavior with inconsistent formatting and location reporting.
3. Costly future migrations if external integrations depend on unstable internals.
4. Growing runtime complexity due to syntax-level branching and stringly typed config paths.

## Decision Record Guidance

When introducing new runtime/compiler behavior, include:

- What invariant is added or changed.
- Which layer owns the behavior (parser/semantic/planner/runtime/tooling).
- How diagnostics are surfaced (code + span + message).
- Compatibility impact on existing workflows and external integrations.
