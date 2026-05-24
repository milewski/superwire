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

- [ ] Create `superwire-types` and move pure shared data types into it.
- [ ] Create `superwire-dsl` and move parser, formatter, validation, DSL diagnostics, structure metadata, and visitors into it.
- [ ] Create `superwire-test-support` and move workflow source templates, fake MCP utilities, schema helpers, and snapshot helpers into it.
- [ ] Create `superwire-macros` and move exported workflow source macros into it.
- [ ] Create `superwire-semantic` and move semantic index, resolver, type inference, execution planning, graph construction, tooling snapshots, provider config semantics, and workflow type schema conversion into it.
- [ ] Create `superwire-mcp` and move MCP config, client, lock-file types, project lock helpers, lock discovery, MCP schema-to-type conversion, and MCP result helpers into it.
- [ ] Create `superwire-protocol` and move executor HTTP/API DTOs and event DTOs into it.
- [ ] Create `superwire-model` and move provider-neutral model interfaces, model schemas, prompt content/assets, tool definitions, finalize call types, and tool-call limits into it.
- [ ] Create `superwire-provider-cersei` and move the Cersei provider implementation into it.
- [ ] Create `superwire-executor-server` and move Axum routes, SSE support, playground serving, `/lsp` websocket bridge, and `serve_executor*` into it.
- [ ] Remove or empty `superwire-core` after all imports are migrated.
- [ ] Run final workspace verification and ensure all checklist items are complete.

## Final acceptance checks

- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo fmt` passes.
- [ ] `cargo tree -p superwire-types` has no parser, executor, LSP, server, MCP HTTP, Axum, or Cersei dependencies.
- [ ] `cargo tree -p superwire-executor` has no `superwire-lsp`, `axum`, `tower`, `tower-http`, `cersei-provider`, or `cersei-types`.
- [ ] Only `superwire-provider-cersei` depends on `cersei-provider` and `cersei-types`.
