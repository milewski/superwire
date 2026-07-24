<p align="center"><img src="/documentation/docs/public/logo-vertical.svg" width="40%" alt="Superwire"></p>

<p align="center">
  <a href="https://github.com/milewski/superwire/actions/workflows/ci.yml"><img src="https://github.com/milewski/superwire/actions/workflows/ci.yml/badge.svg" alt="Build status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/milewski/superwire" alt="MIT license"></a>
</p>

# Superwire

Superwire is a declarative, strongly typed DSL for server-side AI workflows. A `.wire` file defines runtime inputs, secrets, provider/model profiles, scoped MCP capabilities, agent dependencies, and the final JSON shape.

> [!IMPORTANT]
> Superwire is pre-1.0. The DSL and HTTP contracts may change between revisions. Pin Docker deployments to a published `sha-<commit>` tag; use `latest` only for evaluation.

## Choose your path

| Goal | Start here |
| --- | --- |
| Try a workflow visually | [Playground guide](documentation/docs/playground.mdx) |
| Author and run local `.wire` files | [CLI guide](documentation/docs/cli.mdx) |
| Integrate from any backend | [HTTP quickstart](documentation/docs/quickstart.mdx) |
| Integrate a Laravel application | [Laravel guide](documentation/docs/integrations/laravel.mdx) |

## Run the executor

Docker is the shortest supported path. Published images target Linux AMD64 and ARM64.

```bash
docker run --rm -p 13703:13703 rmilewski/superwire:latest
```

Open <http://localhost:13703/playground>. The same process serves the executor API and the WebSocket language server used by the Playground.

There is currently no dedicated health endpoint. Treat a successful Playground request or a harmless `/validate` request as readiness.

## Run the checked hello workflow

Save the [canonical checked workflow](documentation/docs/examples/wire/hello.mdx) as `hello.wire`. It declares a string input, a runtime API-key secret, an OpenAI model profile, one agent with an object-shaped output, and a final `{message: string}` result.

Validate it with the CLI included in the Docker image:

```bash
docker run --rm \
  --entrypoint superwire-cli \
  -v "$PWD:/work" \
  -w /work \
  rmilewski/superwire:latest \
  workflow check hello.wire
```

Expected output:

```text
workflow is valid
```

Then use the [Quickstart](documentation/docs/quickstart.mdx) to run it through Playground, CLI, or HTTP without putting a real credential in source control.

The language is deliberately strict: duplicate or misplaced `match` branches, empty enum/variant types, reserved reference-root bindings, unsupported escapes, and oversized integers fail with diagnostics instead of coercion. Execution also validates object-shaped input/secrets and all dynamic MCP configuration before any permitted MCP network call; see the [syntax reference](documentation/docs/syntax/overview.mdx) and [diagnostics guide](documentation/docs/api-reference/diagnostics.mdx).

## Output surfaces

The final `.wire` `output` block is the same value on every client, but transports wrap it differently:

| Surface | Success value |
| --- | --- |
| `superwire-cli workflow run` | Prints the final workflow object directly. |
| HTTP JSON `POST /execute` | `{ "output": <final workflow object> }` |
| HTTP SSE `POST /execute` | Final value is `data.output` on `workflow_completed`. |
| Laravel | `$result->output` |
| Playground | Displays the final event's `data.output`. |

## What ships

- `superwire-executor`: HTTP/SSE executor, cache, graph, format/validate routes, Playground, and WebSocket LSP endpoint.
- `superwire-cli`: format, check, run, MCP lock, and runtime-variable commands.
- `superwire-lsp`: stdio language server used by editor integrations.
- `integration/superwire-laravel`: PHP 8.4 package for Laravel 12 and 13.
- `editors/intellij`: IntelliJ plugin with bundled LSP.
- `editors/textmate`: syntax highlighting only; it does not provide VS Code diagnostics or completion.

## Native development

Prerequisites:

- Rust 1.94 for the workspace.
- Node.js 22 for the Playground and documentation.
- Java 21 only when building the IntelliJ plugin.

Build the main binaries:

```bash
cargo build --release \
  -p superwire-executor-server \
  -p superwire-cli \
  -p superwire-lsp
```

Run the repository development Playground:

```bash
just playground
```

This starts the executor on port `3000` and Vite on port `3001`. The packaged Docker executor uses port `13703`.

## Current limitations

- No dedicated executor health route is implemented.
- The standalone TextMate package supplies highlighting only; first-class LSP integration is currently the IntelliJ plugin and the built-in Playground client.
- Laravel must point `SUPERWIRE_EXECUTOR_URL` at the executor; use `http://localhost:13703` for the default Docker command.
- Playground source, input JSON, tab/active-tab state, cache settings, and theme persist in browser local storage. Secrets, output, graph data, and event history do not; secrets remain in page memory until reload, and **Copy with secrets** writes them to the system clipboard.

## Security boundaries

- MCP network trust differs by surface. `superwire-cli` deliberately uses `trusted`, so checking, running, or locking an untrusted workflow requires OS/container egress controls. The executor defaults to `disabled`; only its operator can select `--mcp-network-policy` with `disabled`, `public-only`, or `trusted`, and HTTP request bodies cannot opt in. `public-only` permits public HTTP(S) endpoints while rejecting local, private, reserved, metadata, and other non-public destinations; `trusted` also permits private/local HTTP(S) endpoints. LSP discovery remains offline unless client initialization explicitly sets `workspaceTrust.networkMcpDiscovery` to `trusted`. Review workflows and `superwire.lock` before supplying credentials.
- MCP transport is HTTP JSON-RPC only; no policy enables stdio or local-process transports. Both network-enabled modes require absolute HTTP(S) URLs without user information, ignore environment proxies, and reject redirects. `public-only` additionally validates every DNS answer and pins the approved public address set and port. These controls reduce SSRF exposure but do not replace process/container egress policy.
- Model-provider network trust also differs by surface. The executor defaults to `--provider-network-policy built-in-only`, which permits only each driver’s exact built-in endpoint; only its operator can opt into `public-only` or `trusted`, and request bodies cannot override that choice. `public-only` permits custom public HTTPS endpoints; `trusted` also permits custom public/private/local HTTP endpoints and is the deliberate CLI behavior. Custom endpoints reject URL credentials, queries, and fragments, resolve and pin their approved address set before credentials are selected, ignore environment proxies, reject redirects, and use bounded connect/request/response/stream limits. These controls do not replace egress policy.
- Retained, replayed, and live SSE events expose public lifecycle metadata and result-shape summaries, not raw agent/tool/MCP/provider payloads, prompts, contexts, headers, schemas, secrets, or run capabilities. Only `workflow_completed` carries the final workflow output.
- Agent caching stores both structured output and provider context. Redis values can therefore contain rendered prompts and conversation history; isolate and authenticate Redis, use short TTLs, and disable shared caching for sensitive workflows.

Full documentation: <https://docs.superwire.dev>

## License

Superwire is licensed under the [MIT License](LICENSE).
