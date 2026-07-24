# Superwire documentation

This directory is the Mint documentation site for Superwire developers. It starts with a progressive Playground, CLI, HTTP, or Laravel path and keeps complete `.wire` examples in imported checked snippets.

## Requirements

- Node.js 22
- npm
- Rust 1.94 when validating workflow fixtures with a locally built CLI

## Source layout

- `index.mdx`, `introduction.mdx`, `installation.mdx`, `quickstart.mdx`: developer entry journey
- `cli.mdx`, `playground.mdx`, `editor-setup.mdx`, `troubleshooting.mdx`: operational authoring guides
- `core-concepts/`: workflow mental model
- `syntax/`: language reference
- `mcp/`: capability and lockfile usage
- `api-reference/`: executor, events, diagnostics, LSP, and Rust contracts
- `integrations/`: provider and Laravel clients
- `examples/`: progressive pages
- `examples/wire/`: reusable MDX snippets containing exactly one checked `wire` fence
- `scripts/check-wire-fixtures.mjs`: extracts fixture fences and runs `superwire-cli workflow check`
- `scripts/check-release-contracts.mjs`: guards release workflow, generated-artifact, cache, streaming, Laravel, and dependency contracts
- `docs.json`: navigation and site configuration

## Install

```bash
cd documentation/docs
npm ci
```

The package uses the supported `mint` CLI package, not the legacy `mintlify` package.

## Preview

```bash
npm run dev
```

## Validate

Build the CLI once from the repository root:

```bash
cargo build -p superwire-cli
```

Then run documentation checks:

```bash
cd documentation/docs
npm run check:contracts
npm run check:wire
npm run validate
npm run links
npm run a11y
```

`links` checks internal routes, anchors, redirects, and imported snippets. External links are intentionally not part of deterministic CI.

`check:contracts` keeps Docker publish concurrency, immutable image tags, Pages asset inputs, generated-cache ignores, fail-closed cache invalidation, Laravel result boundaries, and the pinned Mint/js-yaml overrides synchronized with their source contracts. `npm test` runs every check above in the same order used for final documentation verification.

## Reuse a checked workflow

Mint supports importing `.mdx` snippets. A fixture such as `examples/wire/hello.mdx` contains one code fence and is imported by a page:

```mdx
import HelloWorkflow from "/examples/wire/hello.mdx";

<HelloWorkflow />
```

Do not copy the workflow into another page. Update the fixture and let every import render the same checked source.

## Writing rules

- Use current `instruction:` syntax.
- Every agent has an object-shaped `output {}` block; schema reuse appears as a typed field.
- Put deterministic application values in tool `bindings`, not model-visible input.
- Use enum-backed/current DSL names and references; dynamic values include the `dynamic.` root.
- Never put literal ellipses or obsolete syntax in a `wire` fence.
- Label incomplete syntax blocks as fragments in prose.
- Document implemented behavior and current limitations directly; derive typed errors/events from the protocol and never promise unimplemented editor clients, health routes, or integration features.
- Keep credentials out of source and examples.
