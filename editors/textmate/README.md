# Superwire TextMate Grammar

This directory contains a TextMate grammar bundle for syntax highlighting of Superwire DSL (`.wire`) files in JetBrains IDEs and other editors that support TextMate grammars.

## Features

The grammar provides syntax highlighting for:

- **Keywords**: `provider`, `schema`, `agent`, `input`, `output`, `secrets`, `for`, `in`
- **Assignments**: `:` in config, schema, and output blocks
- **Data types**: `string`, `number`, `float`, `boolean`, `null`, arrays, tuples, unions
- **String interpolation**: `{{ ... }}` syntax in single-line and multiline strings
- **Multiline strings**: `"""..."""` syntax
- **References**: `agent.name.field`, `input.field`, `schema.Name`, `secrets.key`, `tool.name`
- **Function calls**: `context(...)`, `compact(...)`, `template(...)`, provider model calls like `openai(...)`
- **Comments**: `//` line comments
- **Provider properties**: `driver`, `endpoint`, `api_key`, `models`
- **Agent properties**: `model`, `tools`, `context`, `output`, `prompt`, `inference`

## Installation

### JetBrains IDEs (IntelliJ IDEA, WebStorm, PyCharm, etc.)

1. Open your IDE settings
2. Navigate to **Editor** → **TextMate Bundles**
3. Click the **+** button to add a new bundle
4. Select the `editors/textmate` directory from this repository
5. Click **OK** to apply

The IDE will automatically recognize `.wire` files and apply syntax highlighting.

### Visual Studio Code

1. Copy the `editors/textmate` directory to your VS Code extensions folder:
   - **Windows**: `%USERPROFILE%\.vscode\extensions\superwire`
   - **macOS/Linux**: `~/.vscode/extensions/superwire`
2. Restart VS Code
3. Open any `.wire` file to see syntax highlighting

### Other Editors

For editors that support TextMate grammars (Sublime Text, Atom, etc.), refer to your editor's documentation on installing TextMate bundles.

## Color Themes

The grammar uses standard TextMate scope names, so it will work with any color theme. The following scopes are used:

- `keyword.control.wire` - Keywords like `provider`, `agent`, `schema`
- `entity.name.type.schema.wire` - Declared schema names
- `entity.name.type.schema-reference.wire` - Referenced schema types like `schema.Brief`
- `entity.name.function.wire` - Function names and agent names
- `entity.name.namespace.wire` - Function namespaces in calls like `foo.bar(...)`
- `variable.parameter.wire` - Property names
- `string.quoted.double.wire` - String literals
- `comment.line.double-slash.wire` - Comments
- `keyword.operator.wire` - Operators like `|` and `?.`
- `punctuation.section.arguments.begin.wire` / `punctuation.section.arguments.end.wire` - Function call parentheses

## Example

```wire
provider ollama {
    driver: "ollama"
    endpoint: "http://127.0.0.1:11434"
    models: ["qwen3:8b"]
}

schema Brief {
    summary: string "Short release summary"
    highlights: [string; 3] "Exactly three highlights"
}

input {
    product_name: string
    audience: string
    release_highlights: [string]
}

agent release_summary {
    model: ollama("qwen3:8b")

    prompt: "Write a short release summary for {{ input.product_name }} using {{ input.release_highlights }}"

    output: schema.Brief
}

agent audience_message {
    model: ollama("qwen3:8b")
    context: context(agent.release_summary)

    inference: {
        temperature: 0.2
    }

    prompt: """
        Write a launch message for {{ input.audience }}.
        Summary: {{ agent.release_summary.summary }}
        Highlights: {{ agent.release_summary.highlights }}
    """

    output: string
}

output {
    brief: agent.release_summary
    message: agent.audience_message
}
```

## Contributing

If you find issues with the syntax highlighting or want to add support for new features, please submit a pull request or open an issue.

## License

This grammar is part of the Superwire project and follows the same license.
