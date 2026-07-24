# Superwire TextMate Grammar

This directory contains a TextMate grammar bundle for syntax highlighting of Superwire DSL (`.wire`) files in JetBrains IDEs and other editors that support TextMate grammars.

## Features

The grammar provides syntax highlighting for:

- **Keywords**: declarations, imports, agent directives, calls, loops, match expressions, and context/asset expressions
- **Assignments**: `:` in configuration, typed-field, object, and agent-property blocks
- **Data types**: `string`, `number`, `float`, `boolean`, `object`, `maybe`, arrays, tuples, enums, variants, and unions
- **Literals**: strings, numbers, `true`, `false`, and `null`
- **String interpolation**: `{{ ... }}` syntax in single-line and multiline strings
- **Multiline strings**: `"""..."""` syntax
- **References**: `agent.name.field`, `input.field`, `schema.Name`, `secrets.key`, `tool.name`, `resource.name`, and `prompt.name`
- **Expressions**: function calls, `asset`, `context`, `compact`, variant projections, and fallback operators
- **Comments**: `//` line comments and `///` documentation comments
- **Agent properties**: `model`, `instruction`, `context`, `uses`, `file`, and `output`

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
provider openai from openai {
    endpoint: "http://localhost:1234/v1"
    api_key: "test-api-key"
}

model openai_model from openai {
    id: "model-a"
}

agent research {
    model: model.openai_model
    instruction: "Research the migration plan."
    output {
        value: string
    }
}

agent summarize {
    model: model.openai_model
    context: compact agent.research {
        instruction: "Compact this prior context for a short summary."
    }
    instruction: "Summarize the compacted context."
    output {
        value: string
    }
}

output {
    result: agent.summarize.value
}
```

## Contributing

If you find issues with the syntax highlighting or want to add support for new features, please submit a pull request or open an issue.

## License

This grammar is part of the Superwire project and follows the same license.
