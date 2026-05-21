# Superwire Refactor Task List

## Tracking Rules

- Mark a task `[x]` only when code is implemented, formatted, linted, and covered by relevant tests.
- Leave a task `[ ]` when it has not started.
- Use `[~]` for partially complete work and add a note under "Incomplete Handoff Notes".
- After each completed implementation slice, update this file before moving to the next task.
- Read the `refactor.md` for the full picture of what is being done and why.
- Commit the changes after every task is completed or partially completed.

## Phase 1: First-Class Test Harness

- [x] Create a shared workflow test support API usable by core, executor, LSP, and CLI tests.
  Description: Start with neutral workflow source, cursor, expectation, schema, and snapshot helpers that can be consumed without pulling executor internals into core or LSP.
  Rationale: The refactor touches parser, validator, formatter, runtime, CLI, and LSP behavior. A shared harness gives future agents a single way to express DSL examples and expected behavior before moving large modules.

- [x] Move or wrap existing `parse_inline_workflow!`, `workflow_source!`, LSP inline macros, executor `execute!` macros, and `TestRunner` concepts behind one cohesive testing surface.
  Description: Keep compatibility wrappers for current tests while introducing a shared macro/builder API for new tests.
  Rationale: Existing tests already encode important behavior but each crate has its own helpers. Wrappers avoid a high-risk big-bang migration while allowing new tests to converge.

- [x] Add typed expectation structs for diagnostics, output, provider requests, MCP requests, events, and completions.
  Description: Represent expected assertions as typed data instead of ad hoc strings and JSON traversal.
  Rationale: Typed expectations produce clearer failures and reduce test breakage from incidental message or formatting changes.

- [~] Add snapshot-style assertion helpers for formatter output, graph JSON, semantic index summaries, and lock files.
  Description: Provide stable text/JSON diff helpers that can be reused by formatter, graph, semantic, and lock tests.
  Rationale: Large structural refactors need stable before/after behavior snapshots to prove no behavior changed.

- [~] Add an in-process fake MCP/client abstraction for tests that do not need TCP framing.
  Description: Introduce trait-backed fake MCP interactions for unit and integration tests while preserving real TCP tests for framing coverage.
  Rationale: Most MCP tests care about request/response behavior, not sockets. Removing unnecessary TCP makes tests faster and less flaky.

- [~] Add property-style or table-driven tests for MCP item normalization, binding merge behavior, type compatibility, reference parsing, dependency cycles, and formatter idempotence.
  Description: Cover high-risk pure logic with focused cases outside full workflow execution.
  Rationale: Pure tests run quickly and catch edge cases before expensive runtime or LSP tests are needed.

## Phase 2: Core DSL Validation Split

- [x] Convert `crates/core/src/dsl/validation.rs` into `crates/core/src/dsl/validation/mod.rs`.
  Description: Turn the large validation file into a module directory while preserving `dsl::validation` exports.
  Rationale: This is the main entry point for splitting validation behavior by responsibility without breaking downstream imports.

- [x] Extract validation report types and diagnostic conversion into `validation/report.rs`.
  Description: Move `ValidationReport`, `ValidationIssue`, `ValidationContext`, `SingletonDeclarationKind`, and diagnostic conversion impls into a dedicated report module.
  Rationale: Reporting is a stable API distinct from validation passes, so it can be separated with low behavior risk.

- [x] Extract validation index construction and lookup APIs into `validation/index.rs`.
  Description: Move `ValidationIndex` and related index-building logic into a focused module, preferably with `ValidationIndex::build(...)`.
  Rationale: The index is shared semantic data. Isolating it reduces the largest file and prepares it to become reusable by LSP, CLI, and executor planning.

- [x] Extract duplicate declaration, property, object field, and typed field validation into `validation/duplicates.rs`.
  Description: Move duplicate checks into one domain module with report emission unchanged.
  Rationale: Duplicate detection is mostly independent from type/reference checks and is safe to test in isolation.

- [x] Extract declaration naming validation into `validation/names.rs`.
  Description: Move provider, model, schema, tool, resource, prompt, and agent naming rules into a focused module.
  Rationale: Naming rules are simple policy checks and should not be mixed with semantic reference resolution.

- [x] Extract schema reference, variant, discriminator, and type-expression validation into `validation/schemas.rs`.
  Description: Move schema-specific validation and type-expression traversal into a schema validation module.
  Rationale: Schema behavior changes often affect runtime, LSP, and formatter assumptions, so it needs a clear owner.

- [x] Extract reference validation, projection validation, and secret-reference policy into `validation/references.rs`.
  Description: Move keyword reference validation, projection traversal, and LLM secret-reference rules into a reference module.
  Rationale: References are used across validation, graph generation, runtime planning, hover, completion, and definitions; this logic must become discoverable and reusable.

- [x] Extract agent property, model binding, tool reference, and output validation into `validation/agents.rs`.
  Description: Move agent-specific property and dependency checks into an agent module.
  Rationale: Agent validation is a large independent domain and future agent syntax should not require editing unrelated validation code.

- [x] Extract dynamic declaration validation and dependency-cycle validation into `validation/dynamic.rs`.
  Description: Move dynamic declaration checks and cycle detection for dynamic/agent dependencies into focused graph-oriented validation.
  Rationale: Cycle detection is algorithmically different from field/type checks and benefits from targeted tests.

- [x] Extract tool call binding, fixed binding, and literal type compatibility validation into `validation/tools.rs`.
  Description: Move tool call validation and binding compatibility checks into a tool module.
  Rationale: Tool validation is shared by deterministic tools, imported MCP tools, runtime schema checks, and LSP completions.

- [~] Move reference-specific validation helpers onto `Reference`.
  Description: Attach root keyword interpretation, projection segment access, scope validation, and dependency collection to `Reference`.
  Rationale: The repository rule requires behavior to live on the type that owns the data, and reference helper functions are currently natural methods.

- [~] Move expression traversal helpers onto `Expression`.
  Description: Attach tool reference collection, direct tool-name extraction, agent dependency collection, and secret-reference detection to `Expression`.
  Rationale: Expression traversal is needed by multiple modules and should not be copied through validator, runtime, graph, and LSP code.

- [ ] Promote `ValidationIndex` or equivalent semantic data into a public-but-internal shared core API.
  Description: Expose immutable accessors for declaration/type/reference lookup while keeping mutation internal.
  Rationale: LSP, CLI, validation, and executor currently rebuild overlapping semantic knowledge.

- [ ] Replace stringly diagnostic construction with typed diagnostic builders where behavior naturally belongs to domain types.
  Description: Add methods such as domain-specific issue constructors when the issue belongs to a type like `Reference`, `ToolDeclaration`, or MCP import declarations.
  Rationale: Centralizing diagnostic construction keeps messages and codes consistent across refactors.

- [x] Move large validation scenario tests out of production validation modules.
  Description: Relocate scenario tests into validation test modules or integration-style fixtures using the shared workflow test API.
  Rationale: Production files should not carry thousands of lines of scenario tests, and focused test modules make future changes easier to review.

## Phase 3: Parser, AST, And Formatter Split

- [ ] Split `crates/core/src/dsl/ast.rs` into workflow, declaration, expression, reference, types, tool, MCP, agent, keywords, and span modules.
  Description: Move AST definitions into domain modules and re-export stable public types from `dsl::ast`.
  Rationale: AST changes currently cause high compile and review churn because all data types live in one large namespace.

- [ ] Preserve stable AST re-exports from `dsl::ast` and `dsl::mod`.
  Description: Keep current downstream imports working while internal modules change.
  Rationale: Compatibility re-exports allow structural refactors without forcing every call site to migrate in one change.

- [ ] Keep keyword parsers and renderers centralized in an AST keyword module.
  Description: Move keyword enums and `from_identifier`/`as_str` behavior into one module.
  Rationale: DSL keyword matching must stay enum-based and centralized to avoid raw string comparisons.

- [ ] Split parser visitor logic into declaration, agent, tool, MCP, expression, and type visitor components.
  Description: Break `visitor.rs` into syntax-domain files while preserving parse output.
  Rationale: Syntax additions should touch the relevant parser component instead of a large parse-tree visitor.

- [x] Centralize object-field and MCP binding merge behavior on owning domain types.
  Description: Add methods on MCP import/binding types or a dedicated binding type for shared/local override merging.
  Rationale: Merge order bugs are easy when each caller implements inheritance separately.

- [ ] Split formatter logic into comments, declarations, expressions, types, tools, MCP, and wrapping modules.
  Description: Turn the formatter into domain renderers with shared writer/wrapping primitives.
  Rationale: Formatting behavior is large and fragile, especially comments and wrapping; smaller modules allow focused tests.

- [x] Add formatter idempotence tests for every formatter fixture.
  Description: Assert parse original, format original, parse formatted, and format formatted produce stable output.
  Rationale: Idempotence catches comment preservation and wrapping regressions early.

## Phase 4: Shared Semantic Type And Reference Services

- [ ] Create or complete `core::semantic::index` as the shared semantic model for validation, LSP, CLI, and executor planning.
  Description: Build one semantic model for declarations, type maps, providers/models, tool schemas, MCP imports, and source spans.
  Rationale: Rebuilding similar indexes in multiple crates causes drift and performance cost.

- [ ] Move type-map conversion helpers onto `TypedField`, `TypeExpression`, `Workflow`, or other owning core types.
  Description: Replace editor-specific helper functions with methods on domain types where the data lives.
  Rationale: Type conversion is core DSL behavior, not an LSP-only concern.

- [ ] Introduce a shared `ReferenceResolver` over `SemanticIndex`, `Reference`, and scope data.
  Description: Return typed resolution results for input, secrets, dynamic fields, agent outputs, tools, imports, and models.
  Rationale: Validation, hover, definition, completion, graph, and runtime planning must agree on what a reference means.

- [ ] Make source-span lookup a core service instead of editor-only indexing.
  Description: Provide reusable symbol-to-span lookup from core semantic data.
  Rationale: LSP ranges are an adapter concern; the semantic source location should be produced by core.

- [ ] Add a `SemanticFixture` helper for semantic index, reference resolution, type, and completion assertions.
  Description: Parse workflow snippets, build semantic data, and expose focused assertion methods.
  Rationale: Semantic refactors need compact tests that assert behavior directly without full LSP/runtime setup.

## Phase 5: LSP Document Feature Split

- [x] Split `crates/lsp/src/document/semantic_index.rs` into construction, completions, definitions, scopes, MCP, and type helper modules.
  Description: Keep `DocumentState` behavior intact while moving semantic index responsibilities into focused files.
  Rationale: Editor features currently depend on a single large index implementation, making completion and definition changes risky.

- [x] Split `crates/lsp/src/document/completion.rs` into root, reference, tool, MCP, model, and type-expression completion modules.
  Description: Route completion dispatch through a module tree organized by completion domain.
  Rationale: Completion logic has many context branches and should scale by syntax area.

- [ ] Replace broad line-prefix parsing with parser-aware completion contexts where possible.
  Description: Use AST spans and semantic context for complete syntax, reserving text-prefix heuristics for incomplete syntax.
  Rationale: Parser-aware completions are less sensitive to whitespace, formatting, and partial edits.

- [x] Move LSP tests into focused completion and diagnostics feature modules.
  Description: Split large completion and tool test files into root, reference, MCP, tool, diagnostics, and schema modules.
  Rationale: Smaller test files make failures easier to localize and future feature additions easier to place.

- [x] Add LSP golden tests for common editing workflows.
  Description: Cover incomplete MCP imports, partial `uses` arrays, for-loop destructuring, provider/model editing, and schema variant editing.
  Rationale: LSP behavior is prone to regress during parser and semantic refactors.

## Phase 6: Executor Runtime Split

- [x] Convert `crates/executor/src/runtime.rs` into a runtime module directory.
  Description: Keep the public `WorkflowExecutor` API in `runtime/mod.rs` and move implementation details into focused modules.
  Rationale: Runtime orchestration, validation, tool calls, MCP rendering, and schema handling are currently coupled in one large file.

- [x] Extract workflow building, MCP lock discovery, and workflow preparation into `runtime/build.rs`.
  Description: Isolate constructors such as `from_source` and setup logic that prepares workflows before execution.
  Rationale: Build-time behavior should be testable without running the execution loop.

- [x] Extract runtime input and secrets validation into `runtime/configuration.rs`.
  Description: Move runtime value validation against workflow declarations into a configuration module.
  Rationale: Startup validation is a separate concern from executing agents and tools.

- [x] Extract execution loop and dependency scheduling into `runtime/execution.rs`.
  Description: Move graph scheduling, concurrency, and node execution coordination out of the public runtime module.
  Rationale: Scheduling is the core runtime algorithm and needs focused tests and future performance work.

- [x] Extract single-agent execution into `runtime/agent.rs`.
  Description: Move prompt building, model calls, agent output handling, and agent-specific event emission into an agent module.
  Rationale: Agent execution is complex enough to own its own context and tests.

- [x] Extract for-loop execution into `runtime/for_loop.rs`.
  Description: Move loop item evaluation, scope handling, and loop output aggregation into a dedicated module.
  Rationale: Loop execution has different scope and scheduling rules from normal agents.

- [x] Extract deterministic and model tool-call support into `runtime/tools.rs`.
  Description: Move tool schema construction, fixed bindings, startup calls, and model tool call handling into a tool module.
  Rationale: Tool behavior is shared by runtime execution and semantic planning.

- [x] Extract MCP import prompt/resource rendering and runtime binding merge into `runtime/mcp.rs`.
  Description: Move runtime MCP prompt/resource/tool rendering and binding evaluation into an MCP module.
  Rationale: MCP behavior has its own schema, lock, binding, and request lifecycle concerns.

- [x] Extract schema shaping and output validation into `runtime/schema.rs`.
  Description: Move model request schema generation and output validation into a schema module.
  Rationale: Schema handling is pure enough to test independently and expensive enough to optimize later.

- [x] Move runtime extension traits near owning AST/core types or focused runtime modules.
  Description: Relocate `Reference`, `Expression`, and `TypedToolIr` extension behavior to their owning type modules or runtime domains.
  Rationale: Hidden extension traits in `runtime.rs` violate method locality and make behavior hard to find.

- [ ] Introduce explicit runtime context structs for build, validation, agent run, tool call, and MCP render operations.
  Description: Replace long parameter lists with typed context structs.
  Rationale: Context structs make execution state changes explicit and reduce signature churn.

- [ ] Separate pure planning from async execution.
  Description: Keep dependency planning, type/schema computation, and execution plan construction pure where possible.
  Rationale: Pure planning is easier to benchmark, cache, and test than async execution.

- [ ] Add runtime benchmarks for parsing, validation, planning, prompt rendering, schema resolution, and fake-provider execution.
  Description: Add repeatable benchmark targets for representative small, medium, and large workflows.
  Rationale: Performance refactors need baselines to prove improvement and detect regressions.

- [ ] Reduce cloning in execution hot paths using borrowing or `Arc` where appropriate.
  Description: Audit model requests, tool definitions, prompt strings, schemas, and JSON values for unnecessary cloning.
  Rationale: Large parallel workflows pay for avoidable allocation and cloning costs.

## Phase 7: MCP And Tooling Robustness

- [~] Introduce a shared `McpImportBindings` domain type.
  Description: Own shared/local binding merging, existence checks, JSON evaluation, and diagnostic display for MCP bindings.
  Rationale: Prompt/resource parameters and tool fixed bindings represent the same concept and should not be implemented repeatedly.

- [x] Move prompt required-binding validation onto MCP prompt import or binding types.
  Description: Ask import declarations for effective bindings after batch inheritance instead of reconstructing them externally.
  Rationale: Flattened and nested MCP import views must agree to avoid required-argument bugs.

- [x] Add MCP lock contract tests for normalization, lock application, prompt arguments, schema application, and missing servers.
  Description: Cover lock behavior with structured fixtures and expectations.
  Rationale: MCP lock files are an external contract and need stable regression tests.

- [x] Split `crates/core/src/mcp/lock.rs` into lock module files for apply, validate, project, and name resolution.
  Description: Separate lock persistence, workflow application, validation, and item lookup logic.
  Rationale: Lock behavior spans CLI, runtime, LSP, and core validation, so separate responsibilities reduce accidental regressions.

- [x] Consider introducing an `McpClient` trait for executor and lock discovery fakes.
  Description: Define a trait boundary if tests need in-process MCP clients without concrete HTTP/TCP behavior.
  Rationale: A trait enables faster tests, but should only be added if it does not create unnecessary abstraction.

## Phase 8: CLI Command Split

- [x] Split `crates/cli/src/commands/workflow.rs` into check, run, lock, vars, paths, and JSON modules.
  Description: Move each workflow subcommand and shared command helpers into separate files.
  Rationale: CLI command behavior is currently mixed, making targeted tests and review difficult.

- [x] Move vars-file sample generation onto `TypeExpression` or a shared `SampleValueGenerator`.
  Description: Extract sample value generation from CLI into core or a shared support module.
  Rationale: The behavior is type-expression logic and should be reusable outside the CLI.

- [x] Centralize workflow path collection with focused tests.
  Description: Move target path expansion, sorting, and error handling into a command helper module.
  Rationale: Lock and vars commands both need path collection and should not diverge.

- [x] Add CLI tests through the shared test harness.
  Description: Provide helpers for invoking commands and asserting stdout, stderr, status, and file outputs.
  Rationale: Structured command assertions reduce fixture boilerplate and make CLI regressions easier to diagnose.

## Phase 9: Performance Work

- [ ] Cache parsed workflow, validation report, semantic index, and optional MCP enrichment in a shared workflow document type.
  Description: Introduce a `WorkflowDocument` or similar type that owns source, parse result, validation, semantic index, and enrichment.
  Rationale: Many features parse and index the same source repeatedly.

- [x] Avoid rebuilding LSP semantic index when text has not changed.
  Description: Ensure completions, hovers, diagnostics, definitions, symbols, and folding read from cached document state.
  Rationale: LSP responsiveness depends on avoiding repeated expensive work.

- [ ] Replace repeated linear scans with maps in hot semantic paths.
  Description: Use maps for declaration, tool, schema, model, provider, graph node, and completion lookup where order is not the main requirement.
  Rationale: Large workflows make repeated scans expensive.

- [ ] Use `BTreeMap` only when deterministic ordering is needed.
  Description: Keep `BTreeMap` for stable output and tests, prefer `HashMap` for unordered hot paths.
  Rationale: Determinism is valuable at boundaries, but runtime and semantic hot paths should avoid unnecessary costs.

- [ ] Add prefix indexes for completions only if benchmarks show filtering is a bottleneck.
  Description: Start with clean maps and add trie/prefix indexes only after measurement.
  Rationale: Avoid premature complexity until completion filtering is proven slow.

- [ ] Reduce `serde_json::Value` as an internal transport where static domain types are available.
  Description: Keep JSON values at API/dynamic boundaries and use domain structs internally.
  Rationale: Typed data improves compile-time safety and avoids repeated conversion.

- [ ] Add allocation-conscious schema conversion APIs and cache repeated schema outputs.
  Description: Reuse schema conversion outputs for repeated type expressions where possible.
  Rationale: Schema generation appears across validation, runtime, CLI, and LSP and can become expensive.

## Phase 10: Migration And Verification

- [x] Preserve public re-exports during module splits.
  Description: Re-export moved types and functions from old public module paths.
  Rationale: Structural refactors should not force unrelated call-site migrations.

- [x] Add or identify behavior coverage before every extraction.
  Description: Before moving a module, ensure current behavior is covered by existing or new tests.
  Rationale: Extracting code without coverage makes regressions hard to detect.

- [x] Run `cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings` after code changes.
  Description: Use the same pedantic Clippy profile as CI.
  Rationale: The repo requires lint-clean code before commits.

- [x] Run `cargo fmt` after code changes.
  Description: Format the workspace after implementation changes.
  Rationale: Consistent formatting keeps diffs reviewable.

- [x] Run targeted tests for each completed slice.
  Description: Execute focused tests for modified crates and modules before broader checks.
  Rationale: Targeted tests give faster feedback and isolate failures.

- [x] Commit completed changes without pushing.
  Description: Create a local git commit after verification succeeds or after clearly documenting any remaining blocker.
  Rationale: The user requested a local commit and explicitly said not to push.

## Incomplete Handoff Notes

- Snapshot helpers are only partially complete. `superwire_core::testing::SnapshotAssertion` and `stable_text_diff` now support stable text comparisons, but graph JSON, semantic index summary, and lock-file specific assertions still need typed wrappers and tests.
- Executor support now uses `superwire_core::testing::WorkflowSource` and schema helpers, and core/LSP inline source helpers share the core workflow template API. CLI tests have not been migrated to shared command/test helpers yet.
- Reference/expression method locality is partially complete. `Reference` now owns direct keyword-name extraction, tool/import-name extraction, and agent-dependency collection, while `Expression` owns referenced-name extraction, agent tool binding field access, and pure tool-call traversal; remaining work should move reference path/projection validation and secret-reference detection onto owning AST types or shared semantic services.
- `McpImportBindings` now owns shared/local AST field merging for MCP batch imports. Remaining work should move runtime JSON evaluation and diagnostic display for evaluated bindings into the same domain boundary.
- In-process fake MCP support is partially complete. `McpClientBackend`, `McpClientFactory`, and `FakeMcpClientFactory` now provide a trait-backed fake client path, and LSP MCP discovery tests use it. Remaining work should migrate executor/support MCP tests that only assert request/response behavior away from TCP where practical, while preserving dedicated HTTP/SSE framing tests. CLI workflow lock tests still run through subprocesses and need a separate injection seam before they can use the fake client directly.
- Formatter fixture idempotence coverage is complete. The broader Phase 1 pure-logic test item remains partial because MCP item normalization, binding merge behavior, type compatibility, reference parsing, and dependency cycle table-driven tests are still outstanding.
