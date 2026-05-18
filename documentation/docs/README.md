# Superwire documentation

This directory contains the Mintlify documentation for the Superwire workflow DSL.

The documentation is written for application developers who execute `.wire` files through the Docker executor service.

## Structure

- `introduction.mdx`, `quickstart.mdx`, `installation.mdx`: first-run onboarding around the executor HTTP API
- `why-superwire/`: product positioning, benefits, use cases, comparison, and adoption guidance
- `core-concepts/`: workflow mental model and declaration reference
- `syntax/`: grammar-level DSL reference
- `mcp/`: MCP server, tool, resource, prompt, and batch import usage
- `guides/`: practical workflow authoring conventions
- `api-reference/executor-api.mdx`: `/execute`, `/validate`, and `/format` request/response contract, including event-stream mode via `Accept: text/event-stream`
- `examples/`: complete `.wire` examples
- `docs.json`: Mintlify navigation and site configuration

## Run locally

```bash
cd documentation/docs
npx mintlify dev
```

## Validate the docs

```bash
cd documentation/docs
npx mintlify lint
```

## Writing rules

- Lead with the Docker executor and HTTP payloads.
- Use `instruction:` for agent prompts.
- Use `uses:` for agent tool access.
- Agent outputs are always object blocks, for example `output { answer: string }`.
- Document the current unreleased language directly; avoid historical compatibility notes.
- Keep examples small enough to copy into a single `.wire` file.
