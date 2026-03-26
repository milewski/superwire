# Agent Refactor Rules

## Method Locality (Mandatory)

- Always attach behavior to the struct/enum that owns the data.
- Prefer inherent impl methods on the domain type over free helper functions.
- Do not introduce one-off helper functions when the logic is only used once.
- If a function operates on `FunctionCall`, `Reference`, `Expression`, `AgentDeclaration`, or similar AST/runtime types, implement it on that type first.
- Only create free helper functions when the logic is truly cross-cutting and reused by multiple types/modules.

## Anti-Pattern To Avoid

- Avoid scattered file-level helpers like `do_x(value: &SomeStruct)` that are called from one place and mirror a natural method on `SomeStruct`.

## Review Checklist

- Before finishing a change, verify: “Can this helper become a method on the owning type?”
- If yes, refactor it into the owning type before finalizing.

# Code Style Guidelines

## Variable Naming

**NEVER use abbreviated or single-letter variable names.** Always use descriptive, full names that clearly indicate the
variable's purpose.

Avoid: `s`, `i`, `ctx`, `cfg`, `msg`, `res`, `err`

Use instead: `message`, `index`, `context`, `config`, `response`, `error`

## Block Spacing

**Always add a blank line between blocks.** This includes separating match statements, if blocks, for loops, while
loops, and other control flow structures from each other and from surrounding code.

This improves readability by creating visual separation between logical sections of code.

## Variable Assignment Spacing

**Consecutive single-line variable assignments should not have blank lines between them.** Only add a blank line when
the assignment chain is broken by a multi-line expression or when transitioning to a different logical section.

Good:

```rust
let schema = self.extract_schema(agent)?;
let done_tool = Arc::new(DoneTool::new(schema.clone()));
let mut context = Vec::new();
```

Bad:

```rust
let schema = self.extract_schema(agent)?;

let done_tool = Arc::new(DoneTool::new(schema.clone()));

let mut context = Vec::new();
```

Exception: When an assignment contains a long chain of elements that spans multiple lines, add a blank line before the
next assignment:

```rust
let schema = self.extract_schema(agent)?;
let done_tool = Arc::new(DoneTool::new(schema.clone()));

let complex_value = some_function()
    .with_option_one()
    .with_option_two()
    .with_option_three()
    .build()?;

let next_value = another_assignment();
```

## Documentation Files

**Do not create documentation, explanation, or implementation detail files unless explicitly requested.** This includes
files like README.md, IMPLEMENTATION.md, ARCHITECTURE.md, or any other files that explain functionality or design
decisions.

Only create such files when the user specifically asks for them.

## Type Safety

**Always use enums instead of hardcoded strings for fixed sets of values.** This provides compile-time type safety and
prevents typos and invalid values.

Avoid: `"success"`, `"fail"`, `"pending"`, `"completed"`

Use instead: Define an enum like `Status::Success`, `Status::Fail`, `Status::Pending`, `Status::Completed`

This applies to all cases where a value can only be one of a fixed set of options, including status codes, operation
types, configuration modes, and any other categorical data.

## DSL Keyword Matching (Mandatory)

**Never match DSL language keywords with raw string comparisons.** Always resolve keywords through DSL enums and match
against enum variants.

Use enum parsers and renderers such as:

- `DeclarationKeyword::from_identifier(...)`
- `AgentExpressionPropertyName::from_identifier(...)`
- `ReferenceKeyword::from_identifier(...)`
- `BuiltinFunctionName::from_identifier(...)`
- `*.as_str()` when rendering output

Bad:

```rust
if property_name == "context" {
    // ...
}

if root == "agent" || root == "input" {
    // ...
}
```

Good:

```rust
if property_name == AgentExpressionPropertyName::Context {
    // ...
}

match ReferenceKeyword::from_identifier(root_identifier) {
    Some(ReferenceKeyword::Agent) | Some(ReferenceKeyword::Input) => {
        // ...
    }
    _ => {
        // ...
    }
}
```

This rule is mandatory for all parser, validator, runtime, and LSP completion/hover logic.

## Code Quality

**Always run formatting and linting after making code changes.** Run the same pedantic Clippy profile used by CI before
every commit so local checks catch failures early.

Use the following commands:

```bash
cargo clippy --fix --allow-dirty --all-targets --all-features -- -D warnings
cargo fmt
```

This ensures consistent code formatting and catches common mistakes automatically.
