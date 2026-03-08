# AI DSL TextMate Grammar

This directory contains a TextMate grammar bundle for syntax highlighting of AI Engine DSL (`.ai`) files in JetBrains IDEs and other editors that support TextMate grammars.

## Features

The grammar provides syntax highlighting for:

- **Keywords**: `provider`, `schema`, `agent`, `input`, `output`, `for_each`, `as`
- **Operators**: `<-` (assignment), `:` (type annotation)
- **Terminal marker**: `<-` prefix for terminal agents
- **Data types**: `string`, `number`, `boolean`, `null`, arrays, enums
- **String interpolation**: `{{ variable }}` syntax
- **Multiline strings**: `"""..."""` syntax
- **References**: `agent.name.field`, `input.field`, `schema.name`
- **Function calls**: `file`, `compact`
- **Comments**: `//` line comments
- **Provider properties**: `driver`, `api_endpoint`, `models`
- **Agent properties**: `model`, `tools`, `context`, `output`, `prompt`, `for_each`

## Installation

### JetBrains IDEs (IntelliJ IDEA, WebStorm, PyCharm, etc.)

1. Open your IDE settings
2. Navigate to **Editor** → **TextMate Bundles**
3. Click the **+** button to add a new bundle
4. Select the `editors/textmate` directory from this repository
5. Click **OK** to apply

The IDE will automatically recognize `.ai` files and apply syntax highlighting.

### Visual Studio Code

1. Copy the `editors/textmate` directory to your VS Code extensions folder:
   - **Windows**: `%USERPROFILE%\.vscode\extensions\ai-dsl`
   - **macOS/Linux**: `~/.vscode/extensions/ai-dsl`
2. Restart VS Code
3. Open any `.ai` file to see syntax highlighting

### Other Editors

For editors that support TextMate grammars (Sublime Text, Atom, etc.), refer to your editor's documentation on installing TextMate bundles.

## Color Themes

The grammar uses standard TextMate scope names, so it will work with any color theme. The following scopes are used:

- `keyword.control.ai` - Keywords like `provider`, `agent`, `schema`
- `entity.name.type.ai` - Type names and schema names
- `entity.name.function.ai` - Function names and agent names
- `variable.parameter.ai` - Property names
- `string.quoted.double.ai` - String literals
- `comment.line.double-slash.ai` - Comments
- `keyword.operator.assignment.ai` - Assignment operator `<-`

## Example

```ai
provider ollama1 {
    driver <- "ollama"
    api_endpoint <- "http://localhost:11434"
    models <- ["qwen3:8b"]
}

input {
    topic: string
    audience: string
}

agent research {
    model <- "ollama1/qwen3:8b"

    output <- {
        summary: string
        key_points: [string]
    }

    prompt <- "Research {{ input.topic }} for {{ input.audience }}"
}

<- agent report {
    model <- "ollama1/qwen3:8b"

    for_each <- agent.research.key_points as point

    prompt <- """
        Expand on this key point: {{ input.point }}
        Write a detailed paragraph.
    """
}

output {
    topic <- input.topic
    summary <- agent.research.summary
}
```

## Contributing

If you find issues with the syntax highlighting or want to add support for new features, please submit a pull request or open an issue.

## License

This grammar is part of the AI Engine DSL project and follows the same license.
