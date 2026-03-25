# Workflow Runtime Task List

This checklist breaks down long-term runtime/compiler work into phased, dependency-aware tasks.

## How To Use

- Keep this file updated as work progresses.
- Mark completed items with `[x]`.
- Add links to PRs/issues under each completed task.
- Do not start tasks that depend on unchecked prerequisites.

## Progress Legend

- `[ ]` not started
- `[~]` in progress (temporary marker if needed)
- `[x]` completed

---

## Phase 0 - Baseline Stability (Current Foundations)

- [x] Enforce strict agent output shape validation (no runtime output recovery)
  - Reasoning: Runtime correctness must remain deterministic and schema-driven.

- [x] Constrain finalize answer shape with runtime-provided schema
  - Reasoning: Prevents model/tool responses from bypassing DSL output contracts.

- [x] Use enum-based inference settings keys
  - Reasoning: Removes stringly typed configuration drift and centralizes extension points.

- [x] Use enum-based provider driver handling
  - Reasoning: Creates typed growth path for future providers and defaults.

- [x] Add workflow fragment composition support (`#fragment;`) in inline workflows
  - Reasoning: Reduces boilerplate and improves test and workflow reuse.

---

## Phase 1 - Compiler Pipeline Separation (High Priority)

- [x] Introduce explicit pipeline boundaries: parse -> normalize -> validate -> typecheck -> plan
  - Reasoning: Separating phases avoids semantic leakage into runtime logic and enables incremental tooling.
  - Deliverable: One orchestration entrypoint with typed output per stage.

- [x] Add typed IR module distinct from parser AST
  - Reasoning: AST tracks syntax; IR tracks resolved semantics and runtime invariants.
  - Deliverable: `core::semantic::ir` with stable structures consumed by planner/runtime.

- [x] Move runtime planning to `ExecutionPlan` generated from typed IR
  - Reasoning: Runtime should execute, not infer semantics ad hoc.
  - Deliverable: Deterministic plan object with resolved provider/agent dependencies and typed edges.

- [x] Add invariant checks at IR/planner boundary
  - Reasoning: Catch impossible states before runtime starts.
  - Deliverable: Planner validation pass with explicit diagnostics.

---

## Phase 2 - Diagnostics Platform (Critical UX and Tooling)

- [x] Create shared diagnostic model
  - Reasoning: Parser/runtime/LSP currently need a unified format for consistency.
  - Deliverable: `Diagnostic { code, severity, message, primary_span, labels, notes, help }`.

- [x] Introduce stable diagnostic codes (for example, `WF1xxx`, `WF2xxx`)
  - Reasoning: Stable codes enable docs, support workflows, and LSP integrations.
  - Deliverable: Diagnostic code registry and mapping policy.

- [x] Implement code-frame renderer with arrows and span labels
  - Reasoning: Rich snippet diagnostics drastically improve debugging speed.
  - Deliverable: CLI renderer showing line numbers, highlights, and pointers.

- [x] Ensure every validation/runtime error maps to diagnostics + spans where possible
  - Reasoning: Avoid generic runtime messages when source location is available.
  - Deliverable: Span propagation strategy from parser AST to IR and runtime failures.

---

## Phase 3 - Formatter and Source Preservation

- [ ] Introduce CST or token-trivia retention model
  - Reasoning: Reliable formatting requires comments/whitespace preservation data.
  - Deliverable: Non-lossy parse representation for formatter pipeline.

- [ ] Define formatter style guide and idempotency requirements
  - Reasoning: Predictable formatting is a core user expectation and tooling contract.
  - Deliverable: Style rules + golden tests.

- [ ] Build formatter command and test harness
  - Reasoning: Ensures stable output over large workflow sets.
  - Deliverable: formatter CLI entrypoint + regression fixtures.

---

## Phase 4 - LSP Integration on Shared Semantics

- [ ] Make LSP consume shared semantic compiler pipeline
  - Reasoning: LSP should not duplicate parser/typechecker logic.
  - Deliverable: LSP diagnostics backed by shared compiler artifacts.

- [ ] Add incremental document analysis cache
  - Reasoning: Needed for responsive real-time diagnostics.
  - Deliverable: versioned document cache keyed by URI + version.

- [ ] Implement semantic LSP features on typed IR
  - Reasoning: Hover/completion/go-to-definition require symbol + type resolution.
  - Deliverable: completion for references and fields, hover types, definition navigation.

---

## Phase 5 - FFI and External Integration

- [ ] Define versioned public API boundary (compile, validate, execute)
  - Reasoning: External bindings must not depend on unstable internal structures.
  - Deliverable: Stable API contract and versioning rules.

- [ ] Add C-compatible entrypoints (or another narrow ABI layer)
  - Reasoning: Enables integration with Python/Node/Go and other languages reliably.
  - Deliverable: Thin ABI wrapper around stable boundary API.

- [ ] Serialize diagnostics/results through stable transport schema
  - Reasoning: Language boundaries need deterministic data contracts.
  - Deliverable: JSON schema (or equivalent) for compile diagnostics and execution outputs.

---

## Cross-Cutting Hardening Tasks

- [ ] Add architecture tests for phase boundaries
  - Reasoning: Prevent accidental coupling between parser, semantic, planner, runtime layers.
  - Deliverable: Tests that fail if forbidden module dependencies appear.

- [ ] Add performance benchmarks for parse/typecheck/plan/execute
  - Reasoning: Early baseline avoids regressions as features expand.
  - Deliverable: Bench suite with representative workflow corpus.

- [ ] Add structured telemetry hooks for compiler and runtime phases
  - Reasoning: Operational visibility is required for production debugging.
  - Deliverable: Timing/error counters by phase.

- [ ] Define migration policy for workflow language changes
  - Reasoning: Backward compatibility becomes harder over time without a policy.
  - Deliverable: Compatibility matrix and deprecation strategy.

---

## Suggested Execution Order

1. Phase 1 (pipeline + IR + planner)
2. Phase 2 (diagnostics)
3. Phase 3 (formatter prerequisites and implementation)
4. Phase 4 (LSP using shared semantics)
5. Phase 5 (FFI stable boundary)

This order minimizes rework by stabilizing semantics and diagnostics before editor and FFI surfaces depend on them.
