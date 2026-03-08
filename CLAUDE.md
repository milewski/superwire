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
