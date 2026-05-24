# Superwire crate split implementation plan

## Goal

Split the unpublished workspace into smaller publishable crates while preserving existing behavior and keeping the full test suite passing. Breaking imports and crate boundaries is acceptable.

## Rules

- Keep functionality unchanged unless a test must be updated only because crate paths changed.
- Keep one implementation step per commit.
- Run `cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings`, then `cargo fmt`, before committing each completed step.
- Do not leave `superwire-executor` depending on `superwire-lsp`, Axum, Tower, Tower HTTP, or Cersei crates after the relevant split steps.
- Do not introduce compatibility facades unless needed temporarily inside the same step.

## Checklist

- [x] Create `superwire-types` and move pure shared data types into it. Rationale: every crate should be able to depend on AST/domain DTOs without pulling parser, semantic analysis, executor, LSP, server, MCP HTTP, or provider dependencies.
- [ ] Create `superwire-dsl` and move parser, formatter, validation, DSL diagnostics, structure metadata, and visitors into it. Rationale: language implementation is publishable and reusable, but it should be separate from pure data definitions.
- [x] Create `superwire-test-support` and move workflow source templates, fake MCP utilities, schema helpers, and snapshot helpers into it. Rationale: shared test infrastructure is valuable across crates but should not be part of production runtime APIs.
- [x] Create `superwire-macros` and move exported workflow source macros into it. Rationale: macros are already a first-class testing and authoring API, and isolating them keeps macro expansion dependencies explicit.
- [x] Create `superwire-semantic` and move semantic index, resolver, type inference, execution planning, graph construction, tooling snapshots, provider config semantics, and workflow type schema conversion into it. Rationale: CLI, LSP, executor, and external tooling need semantic analysis without inheriting unrelated runtime or server code.
- [x] Create `superwire-mcp` and move MCP config, client, lock-file types, project lock helpers, lock discovery, MCP schema-to-type conversion, and MCP result helpers into it. Rationale: MCP is a distinct integration layer used by CLI, LSP, executor, and tests, so it should be publishable and maintainable independently.
- [x] Create `superwire-protocol` and move executor HTTP/API DTOs and event DTOs into it. Rationale: integrations should depend on stable wire contracts without depending on the executor implementation.
- [x] Create `superwire-model` and move provider-neutral model interfaces, model schemas, prompt content/assets, tool definitions, finalize call types, and tool-call limits into it. Rationale: provider authors should implement a small model interface crate instead of depending on executor internals.
- [x] Create `superwire-provider-cersei` and move the Cersei provider implementation into it. Rationale: Cersei is one backend with its own dependencies and should be independently replaceable.
- [x] Create `superwire-executor-server` and move Axum routes, SSE support, playground serving, `/lsp` websocket bridge, and `serve_executor*` into it. Rationale: server transport and playground hosting should not force runtime users to depend on web or LSP crates.
- [ ] Remove or empty `superwire-core` after all imports are migrated. Rationale: the project is unpublished, so a compatibility facade is unnecessary once narrower crates exist.
- [ ] Run final workspace verification and ensure all checklist items are complete. Rationale: this confirms the split did not change behavior and the intended dependency boundaries actually hold.

## Missing or pending tasks

- Owning item: Create `superwire-protocol` and move executor HTTP/API DTOs and event DTOs into it. Reason: executor-owned API request/response DTOs and event DTOs now live in `superwire-protocol`, and executor internals import those types directly from the protocol crate. `GraphResponse` still uses `superwire-semantic::WorkflowExecutionGraph`, so `superwire-protocol` temporarily depends on `superwire-semantic` for the graph wire payload shape while semantic still owns graph construction helpers and graph-specific inherent methods. Follow-up: when graph DTO ownership is revisited, split the graph wire shape from semantic graph construction without moving parser or executor implementation.
- Owning item: Create `superwire-model` and move provider-neutral model interfaces, model schemas, prompt content/assets, tool definitions, finalize call types, and tool-call limits into it. Reason: provider-neutral model DTOs and the provider trait now live in `superwire-model`, but `ModelRequest` still carries MCP client and executor event handles so provider implementations can execute MCP-backed tools and emit executor events without changing behavior. Follow-up: revisit whether provider requests should expose slimmer model-owned tool/event abstractions instead of concrete executor integration handles.
- Owning item: Create `superwire-executor-server` and move Axum routes, SSE support, playground serving, `/lsp` websocket bridge, and `serve_executor*` into it. Reason: server-owned Axum, LSP, default Cersei wiring, and serve helpers now live in `superwire-executor-server`, and provider-backed fixture integration tests moved under `superwire-provider-cersei`. `superwire-executor` no longer depends on `superwire-lsp`, Axum, `superwire-provider-cersei`, `cersei-provider`, or `cersei-types`, but `cargo tree -p superwire-executor -i tower` and `cargo tree -p superwire-executor -i tower-http` still resolve through `jsonschema -> reqwest`, not through server transport code. Follow-up: revisit the schema validation dependency path when slimming runtime/schema dependencies; this is separate from the server split.
- Owning item: Create `superwire-dsl` and move parser, formatter, validation, DSL diagnostics, structure metadata, and visitors into it. Reason: `superwire-dsl` now provides the public DSL API surface and external workspace crates import parser, formatter, validation, diagnostic, AST, and visitor-facing types through it. AST owner types now live in `superwire-types`, semantic analysis now lives in `superwire-semantic`, and MCP integration now lives in `superwire-mcp`, but the parser, formatter, validation driver, and visitors still live in `superwire-core` behind the temporary DSL API crate. Follow-up: move the remaining DSL implementation into `superwire-dsl` and remove the temporary dependency on `superwire-core`.
- Owning item: Create `superwire-test-support` and move workflow source templates, fake MCP utilities, schema helpers, and snapshot helpers into it. Reason: workspace CLI, LSP, executor tests, and test-only executor macros now consume reusable helpers from `superwire-test-support`, and `superwire-dsl::testing` no longer exposes those helpers through a production-facing facade. `superwire-test-support` now depends on `superwire-mcp` for MCP owner types, but it still depends on `superwire-core` for parser-backed workflow fixtures. The original `superwire-core::testing` module remains temporarily for core's own tests while the parser is still owned by core. Follow-up: after `superwire-dsl` owns the parser, delete the legacy core testing module or reduce it to core-private tests only.
- Owning item: Create `superwire-semantic` and move semantic index, resolver, type inference, execution planning, graph construction, tooling snapshots, provider config semantics, and workflow type schema conversion into it. Reason: `superwire-core` temporarily re-exports `superwire-semantic` for core-internal validation and legacy test paths while `superwire-core` still owns parser, validation, and document orchestration. Follow-up: after `superwire-dsl` is extracted, remove the temporary `superwire-core::semantic` re-export.
- Owning item: Remove or empty `superwire-core` after all imports are migrated. Reason: `superwire-core` no longer owns MCP modules, but its temporary `WorkflowDocument` orchestration still accepts `superwire-mcp::McpLock` so LSP document snapshots can apply MCP enrichment while parser and validation remain in core. Follow-up: move document orchestration with the DSL extraction or replace the remaining callers before emptying `superwire-core`.

## Final acceptance checks

- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo fmt` passes.
- [ ] `cargo tree -p superwire-types` has no parser, executor, LSP, server, MCP HTTP, Axum, or Cersei dependencies.
- [ ] `cargo tree -p superwire-executor` has no `superwire-lsp`, `axum`, `tower`, `tower-http`, `cersei-provider`, or `cersei-types`.
- [ ] Only `superwire-provider-cersei` depends on `cersei-provider` and `cersei-types`.
