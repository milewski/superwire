# LSP Refactor Task Plan (Vision-Aligned)

This document is a self-contained handoff plan for improving the LSP implementation while aligning with `WORKFLOW_RUNTIME_LONG_TERM_VISION.md`.

It includes:

- Current-state assessment
- Target architecture decisions
- Ordered, checkbox-based tasks
- File-path level guidance
- Acceptance criteria and verification commands

---

## 1) Context and Goal

Long-term direction (from `WORKFLOW_RUNTIME_LONG_TERM_VISION.md`) requires:

1. One semantic pipeline reused across CLI/runtime/LSP.
2. Diagnostics as first-class, shared structures.
3. Runtime and tooling semantics staying in sync.

Current LSP quality is good and functional, but semantic ownership is still split between `core` and `lsp` in ways that can drift.

---

## 2) Current Code Map (Important Files)

### LSP crate

- `crates/lsp/src/server.rs`
  - JSON-RPC transport/routing, diagnostics publishing, completion/hover responses.
- `crates/lsp/src/protocol.rs`
  - Wire-level LSP request/response models and `DiagnosticCode` enum.
- `crates/lsp/src/document.rs`
  - Main orchestration (`DocumentState`, completion logic, hover logic, parsing fallback, helpers, and many tests).
- `crates/lsp/src/document/scope.rs`
  - Lexical scope scanner for completion context.
- `crates/lsp/src/document/snapshot.rs`
  - Parse/validate snapshot and diagnostic mapping into LSP shape.
- `crates/lsp/src/document/types.rs`
  - LSP-facing internal diagnostic/completion structs.

### Core crate (semantic sources that LSP should rely on)

- `crates/core/src/dsl/ast.rs`
  - Keyword/type/property enums and AST constructs.
- `crates/core/src/dsl/validation.rs`
  - Validation rules and `ValidationIssue` definitions.
- `crates/core/src/semantic/pipeline.rs`
  - Staged parse/validate/typecheck/plan pipeline.
- `crates/core/src/semantic/ir.rs`
  - Typed IR construction and type-driven semantics.
- `crates/core/src/semantic/plan.rs`
  - Execution planning and invariant checks.

---

## 3) Assessment Summary

### What is already strong

- Good enum usage in many places (`DiagnosticCode`, declaration/reference keywords, inference settings).
- Broad LSP test coverage for completion/diagnostics/interpolation/for-loops/tools.
- Major reduction of stringly-typed logic compared to earlier state.

### Main improvement opportunities

1. **Semantic duplication risk**
   - LSP still has local semantic logic (fallback symbol extraction, path resolution, scope heuristics) that can diverge from `core`.
2. **Diagnostics ownership split**
   - Diagnostic mapping is still LSP-side (`document/snapshot.rs`) instead of shared source-of-truth in `core`.
3. **Large module pressure**
   - `crates/lsp/src/document.rs` remains large and mixes concerns.
4. **Server-level robustness/tests**
   - `server.rs` has minimal explicit tests for framing/routing edge cases.

---

## 4) Target Architecture Decisions

These decisions are intended to align with the long-term vision and reduce drift:

1. **Core owns semantics, LSP adapts output**
   - LSP should consume semantic summaries from `core`, not re-implement them.
2. **Core owns diagnostics domain model**
   - Parser/validator/typecheck diagnostics should share one internal representation in `core`.
3. **LSP keeps only editor-specific policies**
   - Trigger chars, insertion formatting, UI-friendly ranking, and transport-specific conversion.
4. **Fallback parsing in LSP is temporary and constrained**
   - Keep fallback lightweight and remove it progressively as tolerant core APIs mature.

---

## 5) Ordered Task Backlog (Checkboxes)

## Phase A — Stabilize and Isolate Current Behavior

- [ ] **A1. Add black-box LSP integration tests at server boundary**
  - **Files**: `crates/lsp/src/server.rs`, new test module under `crates/lsp/src/server_tests.rs` (or `crates/lsp/tests/`).
  - **Goal**: Validate JSON-RPC read/write framing and method routing independent of internal completion implementation.
  - **Acceptance**:
    - `initialize`, `didOpen`, `didChange`, `completion`, `hover`, `didClose` flows are covered.
    - No regressions when `document` internals are refactored.

- [ ] **A2. Add completion behavior matrix tests (table-driven style)**
  - **Files**: `crates/lsp/src/document.rs` test module (later move to `crates/lsp/src/document/tests/`).
  - **Goal**: Explicitly lock behavior for each context: declarations, agent properties, inference block, typed declarations, interpolation, for-loop iterable, tools.
  - **Acceptance**:
    - Table/matrix includes positive and negative cases for each context.

## Phase B — Split `document.rs` by Responsibility

- [ ] **B1. Extract semantic index and path resolution logic into dedicated module**
  - **New files**: `crates/lsp/src/document/semantic_index.rs`, `crates/lsp/src/document/reference.rs`.
  - **Move from**: `crates/lsp/src/document.rs` (`SemanticIndex`, reference path logic, resolve/collect methods).
  - **Goal**: Separate semantic graph/index from request orchestration.

- [ ] **B2. Extract completion engine into dedicated modules**
  - **New files**: `crates/lsp/src/document/completion.rs`, `crates/lsp/src/document/completion_context.rs`.
  - **Move from**: `DocumentState::completion_suggestions` and related context helpers.
  - **Goal**: Keep `DocumentState` as thin coordinator.

- [ ] **B3. Extract hover engine into dedicated module**
  - **New files**: `crates/lsp/src/document/hover.rs`.
  - **Move from**: `hover_markdown` and symbol documentation helpers.
  - **Goal**: Hover behavior changes won’t risk completion logic.

- [ ] **B4. Extract parser/position/text utility helpers**
  - **New files**: `crates/lsp/src/document/text_utils.rs`, `crates/lsp/src/document/position.rs`.
  - **Move from**: `byte_offset_for_position`, token scanners, span/range conversion helpers.

- [ ] **B5. Move tests out of `document.rs` into dedicated test modules**
  - **New folder**: `crates/lsp/src/document/tests/`
  - **Example files**:
    - `completion_tests.rs`
    - `diagnostic_tests.rs`
    - `interpolation_tests.rs`
    - `for_loop_tests.rs`
    - `tool_tests.rs`
  - **Goal**: Keep production module concise and improve maintainability.

## Phase C — Unify Semantic Ownership in Core

- [ ] **C1. Introduce core semantic snapshot API for tooling**
  - **New file**: `crates/core/src/semantic/tooling.rs` (or `crates/core/src/semantic/lsp.rs`)
  - **Expose**:
    - declaration index (providers/schemas/agents)
    - type resolution API for reference paths
    - symbol location spans
    - context-aware lookup helpers
  - **Goal**: LSP consumes this API rather than rebuilding semantics.

- [ ] **C2. Replace `SemanticIndex::from_text_fallback` with core-provided tolerant semantic API**
  - **Files**: `crates/lsp/src/document/*`, `crates/core/src/semantic/*`.
  - **Goal**: Remove or greatly reduce LSP-local fallback heuristics.

- [ ] **C3. Move reference path/type traversal semantics from LSP into core**
  - **Current LSP logic**: `resolve_access_path`, `collect_next_types_for_field`, `collect_available_fields`.
  - **Goal**: one implementation for runtime/compiler/tooling semantics.

## Phase D — Shared Diagnostics Model

- [ ] **D1. Create core diagnostic domain model**
  - **New files**: `crates/core/src/diagnostic/mod.rs` (and optional submodules).
  - **Model should include**:
    - stable code enum
    - severity
    - primary span
    - optional secondary labels
    - notes/help
  - **Goal**: parser/validator/semantic compiler can all emit one shape.

- [ ] **D2. Convert `dsl` parse/validation output to shared diagnostic model**
  - **Files**: `crates/core/src/dsl/parser.rs`, `crates/core/src/dsl/validation.rs`, `crates/core/src/semantic/*`.
  - **Goal**: Remove local code mapping from LSP.

- [ ] **D3. Reduce `crates/lsp/src/document/snapshot.rs` to adapter-only role**
  - **Goal**: only map core diagnostics to LSP wire format (`protocol.rs`) without semantic branching.

## Phase E — Completion Policy and Context Contracts

- [ ] **E1. Define explicit completion policy matrix (source-of-truth table)**
  - **File**: `crates/lsp/src/document/completion_context.rs` or equivalent.
  - **Policy examples**:
    - where builtin functions are allowed
    - where declaration keywords are allowed
    - where only typed values are allowed
    - for-loop iterable constraints
  - **Goal**: remove scattered ad-hoc filters.

- [ ] **E2. Add strict self-reference policy tests**
  - **Already partly covered**; extend to all relevant contexts:
    - `schema` self exclusion
    - `agent` self exclusion (including interpolation and loop contexts)
  - **Goal**: prevent invalid recursive suggestions.

## Phase F — Server and Transport Hardening

- [ ] **F1. Add malformed/partial message tests for `MessageReader`**
  - **File**: `crates/lsp/src/server.rs` tests.
  - **Cases**: missing `Content-Length`, invalid length, partial payload.

- [ ] **F2. Add robust behavior for missing `id` request/notification handling**
  - **File**: `crates/lsp/src/server.rs`.
  - **Goal**: strict JSON-RPC behavior and deterministic responses.

- [ ] **F3. Consider range-based incremental change handling (`didChange`)**
  - **Current**: full-text replacement only.
  - **Goal**: better scalability with editors that send ranged updates.

## Phase G — Performance and Developer Experience

- [ ] **G1. Add lightweight perf smoke benchmarks**
  - **Target**: large workflows completion latency and diagnostics latency.
  - **Files**: benchmark module under `crates/lsp` (if benchmark setup exists).

- [ ] **G2. Add tracing hooks around parse/validate/completion phases**
  - **Files**: `crates/lsp/src/document/*`, `crates/lsp/src/server.rs`.
  - **Goal**: quickly detect slow paths and regressions.

---

## 6) Implementation Constraints and Guardrails

- Keep runtime correctness checks in `core`; avoid introducing semantic rules only in LSP.
- Keep enums as source-of-truth for fixed categories (no new stringly-typed categories).
- Avoid changing user-facing semantics while splitting modules; do behavior-preserving moves first.
- Avoid exposing internal Rust structs across crate boundaries where versioning/stability is required.

---

## 7) Suggested Execution Order (Practical)

1. Phase A (stabilize tests)
2. Phase B (module split)
3. Phase C (core semantic API)
4. Phase D (diagnostic unification)
5. Phase E (policy matrix cleanup)
6. Phase F/G (hardening/perf)

This order minimizes risk: first lock behavior, then refactor internals, then centralize ownership.

---

## 8) Definition of Done

- [ ] LSP completion/hover/diagnostics behavior preserved or intentionally improved with tests.
- [ ] Semantic logic for symbol/type/reference resolution lives primarily in `core`.
- [ ] LSP diagnostic conversion is adapter-only (no semantic branching).
- [ ] `crates/lsp/src/document.rs` reduced to orchestration-focused size.
- [ ] Full validation passes:
  - `cargo test -p engine-ai-lsp`
  - `cargo test -p engine-ai-core`
  - `cargo clippy --fix --allow-dirty`
  - `cargo fmt`
