# Superwire Rust Refactor Plan

This plan is based on a source scan of the `crates` workspace. The main goals are performance, robustness, idiomatic Rust structure, maintainability at scale, and a stronger testing framework that makes whole-system behavior easy to assert from concise workflow snippets.

## Current Hotspots

- `crates/core/src/dsl/validation.rs` is about 5,800 lines and mixes diagnostics, indexing, reference validation, type validation, dependency-cycle validation, and tests.
  This is the largest maintenance risk because unrelated validator changes compete in one file and shared concepts such as references, bindings, and type checks are not isolated behind focused APIs.

- `crates/core/src/dsl/formatter.rs` is about 2,800 lines and owns formatting, comment preservation, expression rendering, wrapping, and declaration-specific formatting.
  This makes formatter behavior hard to test in smaller units and makes it risky to add new syntax because every construct has to be understood in one file.

- `crates/core/src/dsl/ast.rs` is about 2,500 lines and contains all AST structs, enums, keyword enums, rendering helpers, and several domain conversion helpers.
  This gives the project a single large AST namespace, but it also increases compile-time churn and makes it harder to attach behavior to the type that owns the data in a discoverable module.

- `crates/core/src/dsl/visitor.rs` is about 2,300 lines and contains most parse-tree to AST construction.
  It has repeated patterns for batch MCP imports, tool blocks, object fields, typed fields, and property validation that should be centralized as parser visitor components.

- `crates/lsp/src/document/semantic_index.rs` is about 2,700 lines and owns indexing, completion data preparation, definition lookups, type-position checks, dynamic scope lookup, MCP suggestions, and symbol lookup.
  This couples editor features together and duplicates knowledge that already exists in core semantic/type modules.

- `crates/lsp/src/document/completion.rs` is about 1,900 lines and `crates/lsp/src/document/tests/completion_tests.rs` is about 2,200 lines.
  Completion logic has many context-specific branches and test assertions are spread across a large feature file.

- `crates/executor/src/runtime.rs` is about 2,270 lines and owns workflow construction, runtime validation, execution orchestration, tool calling, MCP prompt/resource rendering, loop execution, and runtime extension traits.
  The runtime should become a thin orchestrator over focused components so concurrency, tool calls, and value validation can be tested independently.

- `crates/executor/tests/support/runner.rs` is about 1,200 lines and already contains most ingredients for the desired end-to-end testing framework.
  It should be promoted into a reusable test DSL with macros and builders so tests can express intent without repeating provider/MCP setup and JSON plumbing.

- `crates/cli/src/commands/workflow.rs` is about 1,500 lines and mixes `check`, `run`, `lock`, and `vars` command behavior.
  This is less urgent than core/runtime, but splitting it will reduce CLI regressions and make command tests more focused.

## Refactor Principles

- Keep behavior attached to owning domain types.
  If logic operates on `Workflow`, `Reference`, `Expression`, `FunctionCall`, `McpPromptImportDeclaration`, `ToolDeclaration`, or another AST/runtime type, prefer inherent methods or a narrow extension trait in the same module over free helper functions.

- Extract modules by domain boundary, not by generic utility names.
  Avoid `helpers.rs` or `utils.rs`. Prefer names such as `dsl::validation::references`, `runtime::tool_calls`, `lsp::document::completion::mcp`, and `tests::workflow_dsl`.

- Move tests next to behavior only when they are small unit tests.
  Large scenario suites should move to integration-style fixture or macro modules so production files do not carry thousands of lines of tests.

- Centralize shared semantic concepts in `superwire-core`.
  LSP, CLI, and executor should consume common core APIs for reference resolution, type maps, schema conversion, MCP import binding merging, and diagnostics instead of reimplementing them.

- Prefer typed enums and typed builders for fixed categories.
  Continue avoiding hardcoded DSL keyword string matches. When new test DSLs are added, expose typed expectations for diagnostics, events, tool calls, and MCP calls.

## Phase 1: Build A First-Class Test Harness

- Create `crates/test-support` or `crates/core/src/testing` plus `crates/executor/src/testing` depending on visibility needs.
  The current `crates/executor/tests/support/runner.rs` is powerful but only available to executor integration tests. A dedicated internal test-support crate would let core, executor, LSP, and CLI share inline workflow parsing, lock creation, fake MCP servers, fake model providers, diagnostic assertions, and snapshot helpers without copy/paste.
  This improves robustness because every refactor below can be guarded by concise cross-crate tests.

- Design a macro-first workflow test DSL.
  Target examples:

  ```rust
  workflow_success! {
      source {
          input {
              project_id: number
          }

          output {
              project_id: input.project_id
          }
      }

      input: { "project_id": 42 }
      expect_output: { "project_id": 42 }
  }
  ```

  ```rust
  workflow_error! {
      source {
          agent worker {
              prompt: "hello"
              output { value: string }
          }
      }

      expect_diagnostic: DiagnosticCode::MissingModel
  }
  ```

  ```rust
  workflow_mcp_success! {
      source {
          from mcp.local {
              bindings {
                  project_id: input.project_id
              }

              prompt summary {
                  bindings {
                      type: "task"
                  }
              }
          }
      }

      input: { "project_id": 1 }

      mcp local {
          prompt "summary" {
              argument "project_id" required
              argument "type" required
              text "summary text"
          }
      }

      expect_mcp_request {
          method: "prompts/get"
          params: { "name": "summary", "arguments": { "project_id": "1", "type": "task" } }
      }
  }
  ```

  This improves maintenance because future tests can focus on user-visible behavior instead of constructing `TestRunner`, JSON, model turns, and MCP servers manually.

- Move existing `parse_inline_workflow!`, `workflow_source!`, LSP inline macros, executor `execute!` macros, and `TestRunner` concepts into one cohesive testing API.
  Keep the existing macro names as compatibility wrappers during migration. New tests should use a single set of macros that supports parse, validate, format, execute, graph, LSP diagnostics, LSP completions, and CLI command assertions.

- Add typed expectation structs.
  Suggested types:
  - `ExpectedDiagnostic { code, message_contains, span_text }`
  - `ExpectedOutput(Value)`
  - `ExpectedProviderRequest { provider, model, prompt_contains, tools }`
  - `ExpectedMcpRequest { server, method, params }`
  - `ExpectedEvent { kind, agent_name, tool_name }`
  - `ExpectedCompletion { label, kind, detail_contains }`

  This improves robustness because tests will fail with structured diffs instead of ad hoc string contains and manual JSON traversal.

- Add snapshot-style assertions for formatter and graph output.
  Formatter tests already use fixture markdown files under `crates/cli/tests/fixtures/formatter`. Add a small snapshot helper that prints stable diffs and can be reused for execution graph JSON, semantic index summaries, and lock files.

- Create a minimal fake MCP/server abstraction that does not require real TCP unless the test is specifically validating HTTP framing.
  Current executor tests spin local servers in `runner.rs`. Keep that for integration coverage, but add in-process fake clients behind traits so most tests can avoid sockets, be faster, and be less flaky.

- Add property-style and table-driven tests around high-risk pure logic.
  Good targets:
  - `McpServerLock::normalize_item_name`
  - MCP shared/local binding merge behavior
  - type compatibility and nullability
  - reference parsing and projection
  - dependency cycle detection
  - formatter idempotence

  This improves performance and robustness because pure unit tests run quickly and catch edge cases without executing a whole workflow.

## Phase 2: Split Core DSL Validation

- Convert `crates/core/src/dsl/validation.rs` into `crates/core/src/dsl/validation/mod.rs` with focused submodules.
  Suggested layout:
  - `report.rs`: `ValidationReport`, `ValidationIssue`, diagnostic conversion.
  - `index.rs`: `ValidationIndex` construction and lookup methods.
  - `duplicates.rs`: duplicate declarations, properties, object fields, typed fields.
  - `names.rs`: snake-case and declaration naming rules.
  - `schemas.rs`: schema references, variant/discriminator validation, type-expression validation.
  - `references.rs`: keyword reference validation, projection validation, secret-reference policy.
  - `agents.rs`: agent properties, model bindings, tool references, output validation.
  - `dynamic.rs`: dynamic declaration validation and dependency cycles.
  - `tools.rs`: tool call binding validation, fixed binding rules, literal type compatibility.

  This improves maintainability because each validator can be changed and tested independently. It also reduces review risk for new syntax or new diagnostic rules.

- Move `Reference`-specific validation helpers onto `Reference`.
  The current file has validation behavior that naturally belongs to `Reference`, such as root keyword interpretation, projection traversal, and secret-reference checks. Implement methods such as:
  - `Reference::root_keyword() -> Option<ReferenceKeyword>`
  - `Reference::projection_segments()`
  - `Reference::validate_against_scope(...)`
  - `Reference::collect_agent_dependencies(...)`

  This follows the method-locality rule and reduces scattered free helper functions.

- Move expression traversal behavior onto `Expression`.
  Existing collector traits such as `ToolReferenceCollector`, `DirectToolName`, and dependency collection can become `Expression` methods or narrow visitor traits:
  - `Expression::collect_tool_references(...)`
  - `Expression::direct_tool_name()`
  - `Expression::collect_agent_dependencies(...)`
  - `Expression::references_secret()`

  This makes traversal reusable by validator, semantic graph, LSP, and runtime without repeated recursion.

- Make `ValidationIndex` a public-but-internal semantic product.
  LSP and CLI need much of the same symbol/type information. Promote a cleaned-up index into `core::semantic` or keep it in `core::dsl::validation` with accessor methods. Avoid exposing mutable internals.
  This improves scale because new consumers do not rebuild partial indexes.

- Replace stringly diagnostic construction with typed diagnostic builders.
  Keep `ValidationIssue` as the stable enum, but add methods on domain types to produce issues. For example, `ToolDeclaration::duplicate_binding_issue(...)` or `Reference::unknown_root_issue(...)` where appropriate.
  This reduces repeated message construction and keeps diagnostics consistent.

- Move validation tests out of the production file into `crates/core/src/dsl/validation/tests/`.
  Use the new workflow test macros. Keep small unit tests next to tiny pure functions if useful, but large scenario tests should not live at the bottom of the implementation file.

## Phase 3: Split Parser, AST, And Formatter By Syntax Domain

- Split `crates/core/src/dsl/ast.rs` into domain modules.
  Suggested layout:
  - `ast/workflow.rs`
  - `ast/declaration.rs`
  - `ast/expression.rs`
  - `ast/reference.rs`
  - `ast/types.rs`
  - `ast/tool.rs`
  - `ast/mcp.rs`
  - `ast/agent.rs`
  - `ast/keywords.rs`
  - `ast/span.rs`

  Re-export stable public types from `dsl::ast` and `dsl::mod` so downstream code does not need a large migration all at once.
  This improves compile-time locality and discoverability.

- Keep keyword parsers/renderers in one `keywords.rs` module.
  The project already has good enum-based DSL keyword matching. Centralizing keyword enums makes it harder to accidentally reintroduce raw string matching.

- Extract parse visitor components from `visitor.rs`.
  Suggested layout:
  - `visitor/mod.rs`: top-level dispatch and shared `DslVisitor`.
  - `visitor/declarations.rs`: provider, model, schema, input, output.
  - `visitor/agents.rs`: agent declarations and for-loop parsing.
  - `visitor/tools.rs`: tool declarations, tool imports, tool calls.
  - `visitor/mcp.rs`: MCP imports, MCP batch imports, MCP calls.
  - `visitor/expressions.rs`: expressions, references, literals.
  - `visitor/types.rs`: type expressions and typed fields.

  This improves maintainability because syntax additions only touch the relevant visitor component.

- Centralize object-field merging on owning MCP types.
  There are repeated merge-with-overrides implementations for tools, resources, and prompts. Introduce methods such as:
  - `ObjectField::merge_with_overrides(shared, local)`
  - or `McpPromptBatchImportItem::merged_parameters(shared)`
  - or a domain type like `BindingFields::merge_with_overrides(...)`

  Prefer a method on the owning domain type if behavior is specific to MCP import parameters. This reduces duplication and prevents merge-order bugs.

- Split formatter into declaration and expression renderers.
  Suggested layout:
  - `formatter/mod.rs`: `format_workflow_source`, `DslFormatter`, shared writer primitives.
  - `formatter/comments.rs`: comment preservation.
  - `formatter/declarations.rs`: declaration rendering.
  - `formatter/expressions.rs`: expression rendering and inline logic.
  - `formatter/types.rs`: type rendering.
  - `formatter/tools.rs`: tool call and tool declaration rendering.
  - `formatter/mcp.rs`: MCP imports and MCP calls.
  - `formatter/wrap.rs`: line wrapping and multiline strings.

  This improves testability because each formatter unit can be covered with focused snapshots, and it limits the blast radius of syntax-specific formatting changes.

- Add formatter idempotence tests to the new test harness.
  Every formatter fixture should assert:
  - parse original succeeds
  - format original succeeds
  - parse formatted succeeds
  - formatting formatted output produces the same output

  This improves robustness because comment preservation and wrapping changes will be caught immediately.

## Phase 4: Centralize Semantic Type And Reference Services

- Create `core::semantic::index` as the shared semantic model for LSP, CLI, executor planning, and validation.
  It should expose:
  - declarations by name
  - type maps for input, secrets, dynamic, agent outputs, schema fields
  - model/provider summaries
  - tool binding and input schemas
  - MCP import summaries
  - source spans for definitions

  This reduces duplicated indexing in `dsl::validation`, `lsp::document::semantic_index`, and semantic graph generation.

- Move type-map conversion helpers into core semantic types.
  LSP has helpers such as `typed_fields_to_map`, `typed_fields_to_metadata_map`, and `field_metadata_from_type_map`. Similar concepts exist in core validation and CLI.
  Add methods such as:
  - `TypedField::fields_to_type_map(...)`
  - `TypeExpression::to_workflow_type(...)`
  - `Workflow::semantic_index(...)`

  This improves idiomatic design because behavior sits with the data type rather than in editor-specific helpers.

- Introduce a shared reference resolver.
  The resolver should accept a `SemanticIndex`, a `Reference`, and a scope description, then return a typed resolution result:
  - `ReferenceResolution::InputField`
  - `ReferenceResolution::SecretField`
  - `ReferenceResolution::DynamicField`
  - `ReferenceResolution::AgentOutput`
  - `ReferenceResolution::Tool`
  - `ReferenceResolution::PromptImport`
  - `ReferenceResolution::ResourceImport`
  - `ReferenceResolution::Model`

  This improves robustness because validation, hover, definition, completion, graph edges, and runtime planning can agree on what a reference means.

- Make source-span lookup a core service.
  LSP definition and hover currently depend on editor-side indexing. Core already knows AST spans. Put symbol-to-span resolution behind a reusable API and let LSP adapt the result into LSP ranges.

- Add a `SemanticFixture` test helper.
  It should parse a workflow snippet, build the semantic index, and allow assertions like:
  - `expect_reference("agent.worker.output.value").resolves_to_agent_output("worker", "value")`
  - `expect_type("dynamic.summary").is_string()`
  - `expect_completion_at_marker("/*cursor*/").contains(...)`

  This makes semantic refactors safe and gives LSP tests a smaller dependency surface.

## Phase 5: Split LSP Document Features

- Split `crates/lsp/src/document/semantic_index.rs`.
  Suggested layout:
  - `document/semantic_index/mod.rs`: type definitions and construction.
  - `document/semantic_index/completions.rs`: completion data extraction.
  - `document/semantic_index/definitions.rs`: definition span lookup.
  - `document/semantic_index/scopes.rs`: dynamic/for-loop scope lookup.
  - `document/semantic_index/mcp.rs`: MCP lock-backed suggestions.
  - `document/semantic_index/types.rs`: type-position helpers.

  This improves maintainability because completion changes do not need to touch definition and scope logic.

- Split `crates/lsp/src/document/completion.rs` by completion domain.
  Suggested layout:
  - `completion/mod.rs`: main dispatch.
  - `completion/root.rs`: root declaration completions.
  - `completion/reference.rs`: reference completions.
  - `completion/tool.rs`: tool call and binding completions.
  - `completion/mcp.rs`: MCP import completions.
  - `completion/model.rs`: provider/model completions.
  - `completion/type_expression.rs`: type completions.

  This improves scale because adding new syntax should add one completion module and a focused test file.

- Replace broad line-prefix parsing with parser-aware contexts where possible.
  Completion currently relies heavily on text prefix and scope heuristics. Use `completion_context` plus AST spans and semantic index data to decide context. Keep text-prefix parsing only for incomplete syntax where the parser cannot produce AST.

  This improves correctness because completions will be less sensitive to whitespace and formatting.

- Move LSP tests into feature modules.
  Current completion and tool tests are large. Suggested layout:
  - `document/tests/completion/root.rs`
  - `document/tests/completion/references.rs`
  - `document/tests/completion/mcp.rs`
  - `document/tests/completion/tools.rs`
  - `document/tests/diagnostics/validation.rs`
  - `document/tests/diagnostics/mcp_schema.rs`

  Use shared inline macros from the new test harness.

- Add LSP golden tests for common editing workflows.
  Examples:
  - incomplete `from mcp.local { prompt ... }` import
  - partial agent `uses` array
  - for-loop destructuring
  - model/provider declaration editing
  - schema variant editing

  This improves robustness because LSP behavior tends to break during parser/semantic refactors.

## Phase 6: Split Executor Runtime Into Focused Components

- Convert `crates/executor/src/runtime.rs` into a module directory.
  Suggested layout:
  - `runtime/mod.rs`: `WorkflowExecutor` public API and orchestration.
  - `runtime/build.rs`: `from_source`, MCP lock discovery, workflow preparation.
  - `runtime/configuration.rs`: runtime input/secrets validation.
  - `runtime/execution.rs`: execution loop and dependency scheduling.
  - `runtime/agent.rs`: single agent execution.
  - `runtime/for_loop.rs`: for-loop execution.
  - `runtime/tools.rs`: deterministic and model tool call support.
  - `runtime/mcp.rs`: MCP import prompt/resource rendering and runtime binding merge.
  - `runtime/schema.rs`: model input/output schema shaping and output validation.
  - `runtime/errors.rs`: executor error helpers if needed.

  This improves maintainability because runtime changes will be isolated by execution responsibility.

- Move extension traits near owning types or into core.
  Traits currently inside `runtime.rs` include `ToolReferenceExt`, `WorkflowStartupToolCallsExt`, `ExpressionMcpExecutionPlanExt`, `ExpressionMcpExecutionPlanCollectorExt`, `TypedToolModelSchemaExt`, and `TypedToolRuntimeExt`.
  Decide ownership:
  - `Reference` behavior should move to `core::dsl::ast::reference`.
  - `Expression` collection behavior should move to `core::dsl::ast::expression` or `core::semantic`.
  - `TypedToolIr` runtime schema behavior can live in `executor::runtime::tools` if it needs runtime evaluation, or in core semantic tooling if pure.

  This follows method locality and removes hidden helper behavior from the runtime file.

- Introduce explicit runtime contexts.
  Replace long parameter lists with typed context structs:
  - `WorkflowBuildContext`
  - `RuntimeValidationContext`
  - `AgentRunContext`
  - `ToolCallContext`
  - `McpImportRenderContext`

  This improves robustness because adding a new piece of execution state becomes a struct field instead of a chain of signature changes.

- Separate pure planning from async execution.
  Keep graph/planning/type/schema computations in pure functions and modules, then have async execution consume a plan. This improves performance testing because pure planning can be benchmarked and cached.

- Add runtime benchmarks.
  Use `criterion` or a lightweight benchmark target for:
  - parsing and validating a medium workflow
  - building an execution plan
  - rendering prompts with many dynamic dependencies
  - resolving tool schemas with many fixed bindings
  - executing a graph with many independent agents using fake providers

  This gives performance work concrete baselines.

- Reduce cloning in execution hot paths.
  Inspect `ModelRequest`, tool definitions, prompt strings, schemas, and `serde_json::Value` cloning. Use `Arc` for immutable workflow-level data shared across tasks, and borrow where possible in pure functions.
  This improves performance for large workflows and parallel execution.

## Phase 7: Improve MCP And Tooling Robustness

- Create a shared `McpImportBindings` domain type.
  Prompt/resource parameters and tool fixed bindings all represent workflow-supplied runtime values. A typed wrapper can own:
  - merge shared with local overrides
  - check whether a binding exists
  - evaluate bindings into JSON
  - render binding names for diagnostics

  This reduces duplicated logic in parser, lock validation, runtime execution, graph rendering, and LSP completion.

- Move prompt required-binding validation onto MCP prompt import/binding types.
  The validator should ask an import declaration for its effective bindings after batch inheritance is applied. This avoids future bugs where flattened and nested views disagree.

- Add MCP lock contract tests.
  Test prompt/tool/resource name normalization, lock application to workflow, prompt argument validation, schema application, and missing server behavior with the new macros.

- Split `crates/core/src/mcp/lock.rs`.
  Suggested layout:
  - `mcp/lock/mod.rs`: public lock types and re-exports.
  - `mcp/lock/apply.rs`: applying lock schemas to workflows.
  - `mcp/lock/validate.rs`: prompt argument and binding validation.
  - `mcp/lock/project.rs`: project lock file read/write and workflow key/hash.
  - `mcp/lock/name_resolution.rs`: item normalization and lookup.

  This improves maintenance because lock file persistence, schema application, and validation are separate concerns.

- Consider introducing an `McpClient` trait.
  Current `McpClient` is concrete. A trait would let executor and lock discovery use in-process fakes in tests and avoid TCP where not needed.

## Phase 8: Split CLI Commands And Reuse Core Services

- Split `crates/cli/src/commands/workflow.rs`.
  Suggested layout:
  - `commands/workflow/mod.rs`
  - `commands/workflow/check.rs`
  - `commands/workflow/run.rs`
  - `commands/workflow/lock.rs`
  - `commands/workflow/vars.rs`
  - `commands/workflow/paths.rs`
  - `commands/workflow/json.rs`

  This improves maintainability and makes command tests more targeted.

- Move vars-file generation into core or a shared support module.
  `VarsWorkflowCommand::generate_value_from_type_expression` is useful outside the CLI and should be tested as pure logic. Attach behavior to `TypeExpression` or create a `SampleValueGenerator` with explicit options.

- Centralize workflow path collection.
  `LockWorkflowCommand::collect_workflow_paths_for_targets` is reused by vars. Make this a small command helper module with tests for files, directories, globs if supported, sorting, and error messages.

- Add CLI tests through the shared test harness.
  Current CLI tests use fixtures. Keep them, but add helpers for invoking commands and asserting stdout/stderr/status with structured diffs.

## Phase 9: Performance Work After Structural Refactors

- Cache parsed workflow and semantic index together.
  LSP, graph generation, validation, formatting, and CLI commands often parse or index the same source repeatedly. Introduce a `WorkflowDocument` or `ParsedWorkflowDocument` that holds:
  - source text
  - parsed workflow
  - validation report
  - semantic index
  - optional MCP-backed enrichment

  This improves performance and provides one place to handle parse failures and fallback indexing.

- Avoid rebuilding LSP semantic index on every feature request when text has not changed.
  `DocumentState` already stores a snapshot; make sure completions, hovers, diagnostics, definitions, symbols, and folding all read from cached parse/index data. Add tests that verify replacement invalidates the cache.

- Replace repeated linear scans with maps in hot semantic paths.
  Examples:
  - declaration lookup by name
  - tool/schema/model/provider lookup
  - node lookup during graph construction
  - completion filtering by prefix

  This improves responsiveness for large workflows.

- Use `BTreeMap` only where deterministic ordering is needed.
  For runtime hot paths where order does not matter, prefer `HashMap`. Keep `BTreeMap` for stable output, lock file serialization, and deterministic tests.

- Add prefix indexes for completions if large workflows become slow.
  Start with clean maps and benchmarks; only add trie/prefix index structures if benchmark data shows completion filtering is a bottleneck.

- Reduce `serde_json::Value` as an internal transport where types are known.
  Values are appropriate at API boundaries and dynamic runtime data boundaries. Inside planning, schemas, model requests, and typed outputs, prefer domain structs and convert to `Value` at the edge.

- Add allocation-conscious APIs around schemas.
  Schema conversion appears in core semantic support, executor runtime, and CLI. Cache JSON schema outputs for repeated type expressions where possible.

## Phase 10: Repository Hygiene And Migration Strategy

- Do not do a giant mechanical split as one change.
  The safest order is:
  1. Add shared test harness.
  2. Add tests around current behavior.
  3. Extract one module at a time with no behavior change.
  4. Run full formatting, clippy, and targeted tests after each module extraction.

- Preserve public re-exports during module splits.
  Keep existing imports working by re-exporting from old module paths. Clean up call sites after the extraction compiles.

- Use feature-sized pull requests.
  Suggested PR sequence:
  - testing harness foundations
  - validation report/index split
  - validation references/tools split
  - AST keyword/reference/type split
  - formatter comment/wrap split
  - runtime MCP/tools split
  - LSP semantic index split
  - CLI command split

- Keep behavior snapshots before and after extraction.
  Before each extraction, add or identify tests that cover the behavior. The extraction PR should mostly move code and update imports.

- Track performance with simple repeatable commands.
  Add `just bench` or documented cargo bench commands after benchmarks exist. Track representative workflow sizes:
  - small: one input, one agent, one output
  - medium: several agents, dynamic values, tools, MCP imports
  - large: many schemas, many agents, many LSP completion candidates

## High-Priority Task Backlog

- Task: Build the shared workflow test macro crate/module.
  Scope: `crates/test-support` or shared `testing` modules, plus compatibility wrappers for existing macros.
  Done when: core parse/validate tests, executor success/error tests, and LSP diagnostics can use the same inline workflow source representation.
  Impact: highest robustness improvement; enables safe refactors.

- Task: Extract validation report and validation index.
  Scope: `crates/core/src/dsl/validation.rs` into `validation/report.rs` and `validation/index.rs`.
  Done when: no behavior changes, validation tests pass, public API remains compatible.
  Impact: reduces largest file complexity and creates a reusable semantic foundation.

- Task: Extract reference and expression traversal behavior.
  Scope: `Reference` and `Expression` methods in AST/semantic modules; update validator, runtime, graph, and LSP call sites.
  Done when: helper traits/free functions in validation/runtime are removed or reduced, and dependency/tool/reference tests pass.
  Impact: improves idiomatic Rust structure and reduces duplicated traversal logic.

- Task: Introduce `McpImportBindings`.
  Scope: parser AST, MCP lock validation, runtime MCP rendering, LSP completions.
  Done when: shared/local binding merge logic exists in one domain type and prompt/tool/resource code uses it.
  Impact: removes duplication and prevents inheritance/override bugs.

- Task: Split executor runtime MCP and tool call modules.
  Scope: `runtime/mcp.rs`, `runtime/tools.rs`, `runtime/schema.rs`.
  Done when: `runtime.rs` mostly exposes orchestration and public API, while MCP/tool tests still pass.
  Impact: makes runtime behavior easier to reason about and optimize.

- Task: Split LSP semantic index.
  Scope: semantic index construction, completion data, definitions, scopes, MCP helpers.
  Done when: completion, hover, definition, diagnostics, and symbol tests pass with smaller modules.
  Impact: reduces editor feature coupling and makes completions easier to extend.

- Task: Add formatter idempotence and snapshot helpers.
  Scope: CLI formatter fixtures and core formatter tests.
  Done when: all formatter fixtures assert parse-format-parse-format stability.
  Impact: protects formatter refactors and comment preservation.

- Task: Add performance benchmarks.
  Scope: parse, validate, semantic index, execution plan, runtime fake execution, LSP completion.
  Done when: benchmarks run locally and can compare before/after refactors.
  Impact: turns performance work into measurable improvements.

## Testing Framework Design Notes

- Prefer macros for concise test authoring, but keep builders underneath.
  Macros should expand into typed builders so complex tests can drop down to builder APIs without duplicating infrastructure.

- Inline workflow snippets should remain the default.
  Fixtures are good for large examples, but most behavior should be testable with partial or complete snippets embedded directly in the test.

- Support partial workflow snippets.
  The test harness should provide base workflow templates for common declarations. Example:

  ```rust
  workflow_validate! {
      base: default_model_provider

      source {
          agent summarize {
              prompt: "summarize"
              output { summary: string }
          }
      }

      expect_valid
  }
  ```

- Provide clear failure output.
  Every macro should print:
  - rendered workflow source
  - diagnostics with source spans
  - provider/MCP requests
  - events
  - output JSON
  - expected vs actual diff

- Make async execution tests one line at the call site.
  The macro should hide `tokio::test`, fake provider setup, fake MCP setup, and JSON conversion where possible.

- Make LSP cursor markers first-class.
  Use markers such as `<|>` or `/*cursor*/` in snippets, then strip them before parsing. This makes completion/hover/definition tests readable.

- Keep DSL test source macros strict.
  Continue avoiding raw string literals for DSL source in tests. The shared macros should be the only supported way to define inline DSL snippets.

## Expected Outcome

- Faster feature work because behavior is organized by domain instead of by large files.
- Safer refactors because the shared test harness makes whole-system scenarios cheap to write.
- Better runtime and LSP performance because parsed workflows, semantic indexes, schemas, and lookups can be cached and reused.
- More idiomatic Rust because behavior moves onto owning types and modules expose focused domain APIs.
- Lower regression risk because MCP bindings, reference resolution, type conversion, and formatter behavior become centralized and heavily tested.
