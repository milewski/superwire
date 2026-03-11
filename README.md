# Engine AI - Code Quality Improvement Tasks

## High Priority Refactoring Tasks

### Architecture & SOLID Principles
- [x] Extract `AgentExecutor` class from `execute_agent` method (227 lines → multiple focused classes)
- [x] Create `WorkflowExecutor` class to handle workflow execution logic (in progress - needs integration)
- [ ] Create `ValueResolver` trait with implementations for references, interpolations, and function calls
- [ ] Implement validation rule system using Chain of Responsibility pattern
- [ ] Extract schema operations into `SchemaService`
- [ ] Implement provider registration system (remove hardcoded factory)

### Performance Optimizations
- [x] Use `LazyLock` for regex patterns (currently compiled on every call)
- [x] Optimize string allocations in formatter using `String::with_capacity()`
- [ ] Reduce excessive cloning (146 occurrences, many in hot paths)
- [ ] Batch HashMap lookups in context resolver
- [ ] Reduce unnecessary JSON serialization in hot paths

### Code Quality
- [x] Split `execute_agent` method (227 lines) into smaller, focused methods
- [ ] Split `execute_parsed_workflow_with_inputs_and_registry` (225 lines) into smaller methods
- [ ] Refactor `execute_compact_function` (120 lines) for clarity
- [ ] Extract common parsing patterns from `parser/builder.rs`
- [ ] Consolidate error construction using builder/factory pattern
- [ ] Extract message conversion logic from `providers/ollama.rs` into converter class

### Testing
- [ ] Add unit tests for parser module
- [ ] Add unit tests for validator module
- [ ] Add unit tests for context resolver
- [ ] Add error path testing across all modules
- [ ] Add provider testing (ollama, cached)
- [ ] Add formatter module tests
- [ ] Add schema validation tests
- [ ] Add concurrent execution tests for race conditions

### Macros & Code Generation
- [x] Create macro to reduce boilerplate in validation rules (check_duplicates)
- [x] Create macro for span creation (make_span)
- [x] Create validation macros module structure
- [x] Create parser macros module structure
- [ ] Create macro for error construction patterns
- [ ] Create macro for common AST traversal patterns
- [ ] Optimize existing tool and provider macros

## Completed Tasks
- [x] Use `LazyLock` for regex patterns (performance optimization - eliminates repeated regex compilation)
- [x] Extract `AgentExecutor` class from `execute_agent` method (improved Single Responsibility Principle)
- [x] Reduce code duplication in agent execution logic
- [x] Improve separation of concerns in orchestrator
- [x] Optimize string allocations in formatter (11 locations now use `String::with_capacity()`)
- [x] Create `WorkflowExecutor` class structure (needs integration with engine.rs)
- [x] Create parser and validation macro modules
- [x] Implement check_duplicates macro for validation
- [x] Implement make_span macro for AST construction
- [x] Create ValidationRule trait for future rule-based validation

## Current Focus
Created macro infrastructure for parser and validation modules.
Next: Apply macros to reduce code duplication in validator.rs and builder.rs.
