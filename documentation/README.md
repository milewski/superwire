# SuperWire documentation

This directory contains the Mintlify documentation for the SuperWire DSL,
runtime, integrations, examples, and public Rust APIs.

## Structure

- `introduction.mdx`, `quickstart.mdx`, `installation.mdx`: onboarding pages
- `core-concepts/`: the mental model of workflows
- `syntax/`: grammar-level DSL reference
- `advanced/`: execution, context, tools, and diagnostics notes
- `guides/`: practical authoring, testing, and migration help
- `integrations/`: default runtime provider setup
- `examples/`: complete workflow examples
- `api-reference/`: Rust runtime, LSP, and diagnostics reference
- `docs.json`: Mintlify navigation and site configuration

## Run locally

```bash
cd documentation
npx mintlify dev
```

## Validate the docs

```bash
cd documentation
npx mintlify lint
```

## Writing rules for this docs set

- Prefer real syntax from `crates/core/src/dsl/grammar.pest`
- Prefer runnable snippets from `crates/core/workflows/*.ai`
- Call out parser, validator, and runtime differences when they matter
- Avoid placeholder integrations or APIs that the repository does not ship
- Keep links in `docs.json` in sync with the files on disk
