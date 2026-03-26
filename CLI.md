# CLI Implementation Plan

This document is the planning and execution checklist for adding a workflow CLI to this repository.

Status: planning only. Do not start implementation until this document has been reviewed and refined.

## How To Use This Checklist

- Keep this file updated as work progresses.
- Do not start implementation tasks until the planning and refinement phase is complete.
- When a task is fully complete:
  1. run the relevant verification commands
  2. create a git commit for that finished task
  3. change the task from `[ ]` to `[x]`
  4. optionally record the commit SHA under the task
- Use `[~]` only while a task is actively in progress.
- If scope changes, update this file before coding the new work.

## Goal

Build a new CLI, using `clap`, that can:

- check a `.ai` workflow for syntax, validation, and static compilation errors
- format a `.ai` workflow with one canonical pretty style
- run a workflow directly from a `.ai` file
- build a `.ai` workflow into an executable whose workflow inputs become CLI flags automatically

## Current State Findings

These findings come from the current `crates/core` implementation and should shape the design:

- Parsing already exists in `crates/core/src/dsl/parser.rs` and produces AST nodes with source spans.
- Validation already exists in `crates/core/src/dsl/validation.rs`.
- A staged semantic pipeline already exists in `crates/core/src/semantic/pipeline.rs`: parse -> normalize -> validate -> typecheck -> plan.
- Runtime execution already exists in `crates/core/src/runtime/workflow_runtime.rs`.
- `justfile` already contains placeholder CLI commands, but there is no CLI crate in the workspace yet.
- The parser grammar treats comments as trivia and the AST does not preserve them, so a formatter built only from the current AST would drop comments.
- Runtime execution currently rejects agent `tools` usage as unsupported.
- Runtime state currently has a secrets map, but CLI-facing secret injection is not wired through a public command surface yet.
- Provider parsing currently expects literal string properties such as `api_key`, which affects how secrets should be handled in `run` and `build`.

## Recommended Architecture

- Add a new crate at `crates/cli`.
- Use `clap` derive APIs for the command interface.
- Keep CLI presentation and file IO in the CLI crate.
- Move reusable workflow-domain behavior into `engine-ai-core` when it is not specific to terminal UX.
- Reuse the existing parser, validation, semantic pipeline, diagnostics, and runtime instead of re-implementing workflow semantics in the CLI.
- Add a dynamic compile path in `engine-ai-core` for CLI usage, because the current generic typecheck path is Rust host type oriented.
- Reuse the same input binding and secret resolution logic for both `run` and generated executables from `build`.

## Command Targets

### `check`

- Load a workflow from disk.
- Parse it.
- Validate it.
- Run static semantic compilation without requiring Rust host input/output types.
- Print readable diagnostics with spans.
- Exit non-zero on failure.

### `fmt`

- Parse the workflow into a formatter-friendly representation.
- Discard user formatting and reconstruct the file from scratch.
- Produce one canonical output with no style configuration.
- Default to rewriting the file in place.
- Be idempotent.

### `run`

- Load and compile the workflow from disk.
- Accept workflow input fields as CLI arguments.
- Accept secrets from CLI flags and/or environment variables.
- Execute the workflow through the shared runtime.
- Print the final workflow output as JSON.

### `build`

- Compile a workflow into a standalone executable.
- Make workflow input fields available as CLI flags automatically.
- Reuse the same input coercion and runtime path as `run`.
- Produce a user-invokable binary, not just an intermediate artifact.

## Scope Risks and Constraints

- The formatter cannot safely preserve comments with the current AST alone.
  - Recommended default: preserve comments before shipping `fmt`.
- `tools` workflows cannot run today because runtime explicitly returns an unsupported feature error.
  - Recommended default: allow static checking, but make `run` and `build` fail clearly until tool execution is implemented.
- Provider properties currently expect literal values.
  - Recommended default: either extend provider configuration handling to support secret-backed values or fail early with a clear message for unsupported cases.
- The CLI needs internal workflow typechecking without external Rust schema markers.
  - Recommended default: add a core-owned dynamic compile path instead of making the CLI duplicate type logic.

## Ordered Task Backlog

## Phase 0 - Refine Product Decisions

- [x] Confirm CLI crate, package, and binary naming and update `justfile` expectations.
  - Decision: package `engine-ai-cli`, binary `engine-ai`.

- [x] Confirm formatter comment policy before coding.
  - Decision: `fmt` must not silently delete comments.
  - v1 policy: reject workflows that contain `//` comments with an explicit error and leave files unchanged.

- [x] Confirm `tools` support policy for v1 `run` and `build`.
  - Decision: support them in `check`, fail intentionally in `run` and `build` until tool injection exists.

- [x] Confirm secret input UX for both the host CLI and generated executables.
  - Decision: support both `--secret name=value` and `ENGINE_AI_SECRET_<NAME>` environment variables.

- [x] Confirm build artifact strategy.
  - Decision: generate a small Rust launcher that embeds workflow source and reuses `engine-ai-core`.

## Phase 1 - CLI Foundation and Command Skeleton

- [x] Add `crates/cli` as a new workspace member.
  - Files: root `Cargo.toml`, new `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`.

- [x] Add `clap` and create the top-level command tree.
  - Suggested commands: `check`, `fmt`, `run`, `build`.

- [x] Create CLI module boundaries.
  - Suggested modules: `app.rs`, `commands/check.rs`, `commands/fmt.rs`, `commands/run.rs`, `commands/build.rs`, `diagnostics.rs`, `input.rs`.

- [x] Define stable exit code behavior for success, invalid workflow, runtime failure, and internal error.

- [x] Align `justfile` with the real CLI package name and binary name.

## Phase 2 - Shared Dynamic Compiler Path in Core

- [x] Add a dynamic compile path in `engine-ai-core`.
  - Goal: parse -> validate -> internal typecheck -> plan without Rust generic input/output markers.

- [x] Expose a reusable compiled artifact for CLI commands.
  - Suggested contents: parsed workflow, validation diagnostics, typed IR, execution plan, input type, output type.

- [x] Add CLI-ready diagnostic rendering helpers around shared diagnostics.
  - Reuse `crates/core/src/diagnostic/mod.rs` and source spans instead of inventing new diagnostic models in the CLI crate.

- [x] Add tests for parse, validate, typecheck, and plan failure reporting through the dynamic path.
  - Cover syntax failures, invalid references, missing declarations, bad model bindings, and dependency cycles.

## Phase 3 - `check` Command

- [x] Implement workflow file loading and source-aware diagnostics.

- [x] Wire `check` to the shared dynamic compile path.

- [x] Print a concise success message when the workflow is valid.

- [x] Print readable failures with file path, line, column, and diagnostic message.

- [x] Add integration tests for valid and invalid workflows using samples from `crates/core/workflows/`.

- [x] Verify non-zero exit behavior for CI and scripting use.

## Phase 4 - Formatter Infrastructure and `fmt`

- [x] Decide and implement the formatter source model.
  - Implemented v1 policy: parse AST and rebuild canonically for comment-free workflows.
  - Implemented comment policy: reject `//` comments explicitly and leave the file unchanged.

- [x] Define the canonical style rules for DSL output.
  - Preserve declaration order.
  - Use deterministic indentation and blank-line rules.
  - Use deterministic rendering rules for arrays, objects, call arguments, unions, tuples, string templates, and multiline strings.

- [x] Implement the DSL pretty printer.
  - It must rebuild output from structured data instead of patching whitespace heuristically.

- [x] Implement in-place rewrite behavior for `fmt`.

- [x] Add `fmt --check` mode.
  - Recommended even if it is not the first user-facing flag, because it is useful for CI and generated workflows.

- [x] Add golden tests for formatter output and idempotency.
  - Cover unions, tuples, multiline strings, interpolation, for-loops, tools arrays, and nested objects.

- [x] Add explicit tests for comment preservation or explicitly documented non-preservation behavior.

## Phase 5 - Shared Input and Secret Binding for `run` and `build`

- [ ] Add input coercion from CLI values into `WorkflowType`.
  - Scalars should parse directly.
  - Complex shapes should use JSON-based input handling.

- [ ] Define automatic flag mapping from workflow `input` fields.
  - Example: `input { topic: string }` becomes `--topic <string>`.

- [ ] Define behavior for booleans, arrays, tuples, objects, unions, and null-capable fields.
  - Recommendation: use JSON input for non-scalar shapes.

- [ ] Add a secret resolution layer shared by host CLI and generated executables.
  - Recommendation: merge `--secret name=value` flags with `ENGINE_AI_SECRET_<NAME>` environment variables.

- [ ] Decide whether provider properties may reference secrets in v1.
  - If unsupported, fail early with a targeted diagnostic or runtime message.

## Phase 6 - `run` Command

- [ ] Implement `run` on top of the shared dynamic compile artifact.

- [ ] Reuse automatic input binding from Phase 5.

- [ ] Inject resolved secrets into runtime evaluation.
  - This may require extending runtime and provider configuration handling in `engine-ai-core`.

- [ ] Print workflow output as formatted JSON to stdout.

- [ ] Surface runtime errors with clear context and non-zero exit status.

- [ ] Add integration tests for workflows with no input and workflows with typed input.

- [ ] Add explicit tests for unsupported `tools` workflows.
  - Expected v1 behavior: fail clearly and intentionally.

## Phase 7 - `build` Command

- [ ] Design the generated launcher layout.
  - Recommendation: generate a small Rust project or single-file launcher that embeds the workflow source.

- [ ] Reuse `clap` in the generated executable.
  - Workflow inputs should become flags automatically from the compiled input type.

- [ ] Reuse the same input coercion and secret resolution logic as `run`.
  - Avoid duplicating parsing rules between the host CLI and generated binaries.

- [ ] Implement the build pipeline.
  - Suggested flow: materialize generated source -> run `cargo build --release` -> copy the final binary to `--output`.

- [ ] Define build cache and temporary directory behavior.
  - Recommendation: use a deterministic generated directory under `target/engine-ai-cli/` unless the user overrides it.

- [ ] Add integration tests that build a small sample workflow and execute the produced binary.

- [ ] Add tests for generated `--help` output based on workflow input fields.

## Phase 8 - Hardening and Release Readiness

- [ ] Add end-to-end tests for `check`, `fmt`, `run`, and `build`.

- [ ] Add snapshot tests for diagnostics rendering and formatter output.

- [ ] Make error messages and stdout/stderr usage consistent across commands.

- [ ] Validate cross-platform file path handling for generated builds.

- [ ] Update local automation once the CLI exists.
  - `justfile`, workspace membership, and developer scripts should not point at placeholder package names.

## Proposed File Map

### New CLI crate

- `crates/cli/Cargo.toml`
- `crates/cli/src/main.rs`
- `crates/cli/src/app.rs`
- `crates/cli/src/commands/check.rs`
- `crates/cli/src/commands/fmt.rs`
- `crates/cli/src/commands/run.rs`
- `crates/cli/src/commands/build.rs`
- `crates/cli/src/diagnostics.rs`
- `crates/cli/src/input.rs`
- `crates/cli/tests/`

### Likely core changes

- `crates/core/src/semantic/pipeline.rs`
- `crates/core/src/semantic/mod.rs`
- `crates/core/src/runtime/workflow_runtime.rs`
- `crates/core/src/runtime/provider/mod.rs`
- `crates/core/src/runtime/expression.rs`
- `crates/core/src/dsl/` for formatter-related source preservation and pretty-printing support
- `crates/core/src/diagnostic/mod.rs` for shared CLI-facing diagnostic helpers if needed

## Acceptance Criteria

- [ ] `check` rejects syntax, validation, and static compilation problems with readable diagnostics.
- [ ] `fmt` produces exactly one canonical formatting and is idempotent.
- [ ] `run` executes supported workflows directly from `.ai` files using CLI-provided inputs.
- [ ] `build` produces an executable whose input fields are exposed as CLI flags automatically.
- [ ] Unsupported runtime features fail intentionally and clearly.
- [ ] Formatter comment behavior is explicit and covered by tests.
- [ ] Completed tasks are committed before they are marked done in this file.

## Verification Commands

- [ ] `cargo test -p engine-ai-core`
- [ ] `cargo test -p engine-ai-cli`
- [ ] `cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings`
- [ ] `cargo fmt`
