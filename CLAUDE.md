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

## Code Quality

**Always run formatting and linting after making code changes.** Use the following commands:

```bash
cargo clippy --fix --allow-dirty
cargo fmt
```

This ensures consistent code formatting and catches common mistakes automatically.
